use alloc::{borrow::ToOwned, string::String, sync::Arc, vec::Vec};
use spin::{Mutex, MutexGuard};

use crate::{
    block_cache::{block_cache_sync_all, get_block_cache},
    block_dev::BlockDevice,
    efs::EasyFileSystem,
    layout::{DirEntry, DiskInode, DiskInodeType, DIRENT_SZ},
};

/// Virtual filesystem layer over easy-fs
pub struct Inode {
    // indicate which `DiskInode` is mapping
    block_id: usize,
    block_offset: usize,

    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }

    /// Call a function over a disk inode to read it
    pub fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }

    /// Call a function over a disk inode to modify it
    pub fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }

    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        assert!(disk_inode.is_dir());
        // data of `disk_node` should be array of `Dirent`s
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::new_empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device),
                DIRENT_SZ
            );
            if dirent.name() == name {
                return Some(dirent.inode_number());
            }
        }
        None
    }

    /// Find inode under current inode by name (1 level only)
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Inode::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }

    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            assert!(disk_inode.is_dir());
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v = Vec::with_capacity(file_count);
            let mut dirent = DirEntry::new_empty();
            for i in 0..file_count {
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device),
                    DIRENT_SZ
                );
                v.push(dirent.name().to_owned());
            }
            v
        })
    }

    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }

        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut new_blocks = Vec::new();
        for _ in 0..blocks_needed {
            new_blocks.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, new_blocks, &self.block_device);
    }

    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            assert!(root_inode.is_dir());
            self.find_inode_id(name, root_inode)
        };

        // exist already
        if self.read_disk_inode(op).is_some() {
            return None;
        }

        // 1. alloc inode
        let new_inode_id = fs.alloc_inode();
        // 2. init inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, self.block_device.clone())
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File)
            });
        // 3. modify current inode(root inode): add one more dirent
        self.modify_disk_inode(|root_inode| {
            // append file in dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });
        // 4. return inode
        block_cache_sync_all();
        Some(Arc::new(Self::new(
            new_inode_block_id,
            new_inode_block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock
    }

    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert_eq!(
                data_blocks_dealloc.len(),
                DiskInode::total_blocks(size) as usize
            );
            for data_block in data_blocks_dealloc {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }

    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }

    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            // extend first
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        })
    }
}
