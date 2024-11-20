use alloc::{borrow::ToOwned, string::String, sync::Arc, vec::Vec};
use spin::{Mutex, MutexGuard};

use crate::{
    block_cache::{block_cache_sync_all, get_block_cache},
    block_dev::BlockDevice,
    efs::EasyFileSystem,
    layout::{DirEntry, DiskInode, DiskInodeType, DIRENT_SZ},
};

/// Virtual filesystem layer over easy-fs
#[derive(Clone)]
pub struct Inode {
    inode_id: u32,

    // indicate which `DiskInode` is mapping
    block_id: usize,
    block_offset: usize,

    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        inode_id: u32,
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            inode_id,
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
        // data of `disk_inode` should be array of `Dirent`s
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

    /// Find direntry under a disk inode by pred
    fn get_dirent(
        &self,
        disk_inode: &DiskInode,
        pred: impl Fn(&DirEntry) -> bool,
    ) -> Option<DirEntry> {
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::new_empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device),
                DIRENT_SZ
            );
            if pred(&dirent) {
                return Some(dirent);
            }
        }
        None
    }

    /// Read child direnty by inode_id
    pub fn read_dirent<V>(&self, inode_id: u32, f: impl FnOnce(DirEntry) -> V) -> Option<V> {
        self.read_disk_inode(|disk_inode| {
            self.get_dirent(disk_inode, |d| d.inode_number() == inode_id)
        })
        .map(f)
    }

    /// Find inode under current inode(recursively) by name
    pub fn find(&self, path: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        let mut inode_id = self.inode_id;
        let mut block_id = self.block_id as u32;
        let mut block_offset = self.block_offset;

        for name in path.split('/').filter(|s| !s.is_empty()) {
            let opt_inode_id = get_block_cache(block_id as usize, self.block_device.clone())
                .lock()
                .read(block_offset, |disk_inode: &DiskInode| {
                    if disk_inode.is_file() {
                        return None;
                    }
                    self.find_inode_id(name, disk_inode)
                });
            match opt_inode_id {
                Some(v) => {
                    inode_id = v;
                    (block_id, block_offset) = fs.get_disk_inode_pos(v);
                }
                _ => return None,
            }
        }
        Some(Arc::new(Self::new(
            inode_id,
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
    }

    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            if disk_inode.is_file() {
                return Vec::new();
            }
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
        if new_size <= disk_inode.size {
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
    fn create_inode(&self, name: &str, inode_type: DiskInodeType) -> Option<Arc<Inode>> {
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
                new_inode.initialize(inode_type)
            });
        // 3. modify current inode: add one more dirent
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
        let inode = Self::new(
            new_inode_id,
            new_inode_block_id,
            new_inode_block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        );
        if inode_type == DiskInodeType::Directory {
            let curr_inode_id = self.inode_id;
            inode.modify_disk_inode(|curr_inode| {
                curr_inode.initialize_dir(
                    new_inode_id,
                    curr_inode_id,
                    || fs.alloc_data(),
                    &self.block_device,
                );
            });
        }
        // 4. return inode
        block_cache_sync_all();
        Some(Arc::new(inode))
        // release efs lock
    }

    /// Create regular file under current inode
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        self.create_inode(name, DiskInodeType::File)
    }

    /// Create directory under current inode
    pub fn create_dir(&self, name: &str) -> Option<Arc<Inode>> {
        self.create_inode(name, DiskInodeType::Directory)
    }

    fn clear_locked(&self, fs: &mut EasyFileSystem) {
        self.modify_disk_inode(|disk_inode| {
            assert!(disk_inode.is_file());
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

    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.clear_locked(&mut fs);
    }

    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }

    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            assert!(disk_inode.is_file());
            // extend first
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all(); // MUST sync here to avoid data lost
        size
    }

    /// Get inode id
    pub fn inode_id(&self) -> u32 {
        self.inode_id
    }

    /// Get data size of inode
    pub fn get_size(&self) -> usize {
        self.read_disk_inode(|disk_inode| disk_inode.size as usize)
    }

    /// Is dir?
    pub fn is_dir(&self) -> bool {
        self.read_disk_inode(|disk_inode| disk_inode.is_dir())
    }

    /// Is file?
    pub fn is_file(&self) -> bool {
        self.read_disk_inode(|disk_inode| disk_inode.is_file())
    }

    /// Get link number
    pub fn nlink(&self) -> u32 {
        self.read_disk_inode(|disk_inode| disk_inode.nlink)
    }

    /// Create hard link `name` from `src`
    pub fn link(&self, name: &str, src: &Inode) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();

        let op = |root_inode: &DiskInode| {
            assert!(root_inode.is_dir());
            self.find_inode_id(name, root_inode)
        };
        // exist already
        if self.read_disk_inode(op).is_some() {
            return None;
        }

        // add dirent under self
        self.modify_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            self.increase_size(new_size as u32, disk_inode, &mut fs);
            let dirent = DirEntry::new(name, src.inode_id);
            disk_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });
        // inc src nlink
        src.modify_disk_inode(|disk_inode| disk_inode.nlink += 1);
        Some(Arc::new(Self::clone(src)))
    }

    /// Remove hard link (return if removed successfully)
    pub fn unlink(&self, name: &str) -> bool {
        let mut fs = self.fs.lock();
        // self is dir && "name" exists
        let target = self.modify_disk_inode(|disk_inode| {
            if !disk_inode.is_dir() {
                return None;
            }
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut dirent = DirEntry::new_empty();
            let mut swap = DirEntry::new_empty();
            match (0..file_count).position(|i| {
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device),
                    DIRENT_SZ
                );
                dirent.name() == name
            }) {
                Some(i) => {
                    // target
                    let target_inode_id = dirent.inode_number();
                    let (target_block_id, target_block_offset) =
                        fs.get_disk_inode_pos(target_inode_id);
                    let target = Self::new(
                        target_inode_id,
                        target_block_id,
                        target_block_offset,
                        self.fs.clone(),
                        self.block_device.clone(),
                    );
                    // we don't actually delete i-th, but swap last to i-th, and decrease disk_inode.size only
                    // in real world we should decrease data of this dir's actual space (at proper time?)
                    disk_inode.read_at(
                        (file_count - 1) * DIRENT_SZ,
                        swap.as_bytes_mut(),
                        &self.block_device,
                    );
                    disk_inode.write_at(i * DIRENT_SZ, swap.as_bytes(), &self.block_device);
                    disk_inode.size -= DIRENT_SZ as u32;
                    Some(target)
                }
                _ => None, // no such file
            }
        });
        // clear target's data if link decrease to 0
        if let Some(target) = target {
            if target.modify_disk_inode(|disk_inode| {
                disk_inode.nlink -= 1;
                disk_inode.nlink
            }) == 0
            {
                target.clear_locked(&mut fs);
            }
            true
        } else {
            false
        }
    }
}
