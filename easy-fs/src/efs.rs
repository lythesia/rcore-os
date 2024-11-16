use alloc::sync::Arc;
use spin::Mutex;

use crate::{
    bitmap::Bitmap,
    block_cache::get_block_cache,
    block_dev::BlockDevice,
    layout::{DiskInode, DiskInodeType, SuperBlock},
    vfs::Inode,
    BLOCK_SZ,
};

/// An easy file system on block
pub struct EasyFileSystem {
    /// Real device
    pub block_device: Arc<dyn BlockDevice>,
    /// Inode bitmap
    pub inode_bitmap: Bitmap,
    /// Data bitmap
    pub data_bitmap: Bitmap,
    inode_area_start_block: u32,
    data_area_start_block: u32,
}

type DataBlock = [u8; BLOCK_SZ];
// super_block | inode_bitmap | inode_area | data_bitmap | data_area
impl EasyFileSystem {
    /// create efs given device with `total_blocks` & `inode_bitmap_blocks` specified
    pub fn create(
        block_device: Arc<dyn BlockDevice>,
        total_blocks: u32,
        inode_bitmap_blocks: u32,
    ) -> Arc<Mutex<Self>> {
        let inode_bitmap = Bitmap::new(1, inode_bitmap_blocks as usize);
        // how many inodes
        let inode_num = inode_bitmap.maxmium();
        // blocks for inodes
        let inode_area_blocks =
            (((inode_num * core::mem::size_of::<DiskInode>()) + BLOCK_SZ - 1) / BLOCK_SZ) as u32;
        let inode_total_blocks = inode_bitmap_blocks + inode_area_blocks;

        let data_total_blocks = total_blocks - 1 - inode_total_blocks; // `1` stands for super block

        // Q: 为什么这里是除 4097 而不是 4096？除 4096 不正确吗?
        // A: 希望位图覆盖后面的数据块的前提下数据块尽量多。设数据的位图占据x个块，则该位图能管理的数据块不超过4096x。
        // 数据区域总共data_total_blocks个块，除了数据位图的块剩下都是数据块，也就是位图管理的数据块为data_total_blocks-x个块。
        // 于是有不等式data_total_blocks-x<=4096x，得到x>=data_total_blocks/4097。数据块尽量多也就要求位图块数尽量少，
        // 于是取x的最小整数解也就是data_total_blocks/4097上取整，也就是代码中的表达式。
        let data_bitmap_blocks = (data_total_blocks + 4096) / 4097;
        let data_area_blocks = data_total_blocks - data_bitmap_blocks;
        let data_bitmap = Bitmap::new(1 + inode_total_blocks as usize, data_bitmap_blocks as usize);

        let mut efs = Self {
            block_device: Arc::clone(&block_device),
            inode_bitmap,
            data_bitmap,
            inode_area_start_block: 1 + inode_bitmap_blocks,
            data_area_start_block: 1 + inode_total_blocks + data_bitmap_blocks,
        };

        // clear all blocks
        for i in 0..total_blocks {
            get_block_cache(i as usize, Arc::clone(&block_device))
                .lock()
                .modify(0, |data_block: &mut DataBlock| data_block.fill(0));
        }

        // initialize super block
        get_block_cache(0, Arc::clone(&block_device)).lock().modify(
            0,
            |super_block: &mut SuperBlock| {
                super_block.initialize(
                    total_blocks,
                    inode_bitmap_blocks,
                    inode_area_blocks,
                    data_bitmap_blocks,
                    data_area_blocks,
                )
            },
        );

        // write back immediately
        // create a inode for root node "/"
        assert_eq!(efs.alloc_inode(), 0);
        let (root_inode_block_id, root_inode_offset) = efs.get_disk_inode_pos(0);
        assert_eq!(root_inode_block_id, efs.inode_area_start_block);
        assert_eq!(root_inode_offset, 0);
        get_block_cache(root_inode_block_id as usize, Arc::clone(&block_device))
            .lock()
            .modify(root_inode_offset, |root_inode: &mut DiskInode| {
                root_inode.initialize(DiskInodeType::Directory)
            });

        Arc::new(Mutex::new(efs))
    }

    /// Open a block device as a filesystem
    pub fn open(block_device: Arc<dyn BlockDevice>) -> Arc<Mutex<Self>> {
        // read super block
        get_block_cache(0, Arc::clone(&block_device))
            .lock()
            .read(0, |super_block: &SuperBlock| {
                assert!(super_block.is_valid(), "Error loading EFS!");
                let inode_total_blocks =
                    super_block.inode_bitmap_blocks + super_block.inode_area_blocks;
                let efs = Self {
                    block_device,
                    inode_bitmap: Bitmap::new(1, super_block.inode_bitmap_blocks as usize),
                    data_bitmap: Bitmap::new(
                        1 + inode_total_blocks as usize,
                        super_block.data_bitmap_blocks as usize,
                    ),
                    inode_area_start_block: 1 + super_block.inode_bitmap_blocks,
                    data_area_start_block: 1 + inode_total_blocks + super_block.data_bitmap_blocks,
                };
                Arc::new(Mutex::new(efs))
            })
    }

    /// Get the root inode of the filesystem
    pub fn root_inode(efs: &Arc<Mutex<Self>>) -> Inode {
        let guard = efs.lock();
        let (root_inode_block_id, root_inode_offset) = guard.get_disk_inode_pos(0);
        let block_device = guard.block_device.clone();
        drop(guard);

        Inode::new(
            root_inode_block_id,
            root_inode_offset,
            efs.clone(),
            block_device,
        )
    }

    /// Get inode by id, return (block_id, offset)
    pub fn get_disk_inode_pos(&self, inode_id: u32) -> (u32, usize) {
        const INODE_SIZE: usize = core::mem::size_of::<DiskInode>();
        const INODES_PER_BLOCK: u32 = (BLOCK_SZ / INODE_SIZE) as u32;
        let block_id = self.inode_area_start_block + inode_id / INODES_PER_BLOCK;
        (
            block_id,
            (inode_id % INODES_PER_BLOCK) as usize * INODE_SIZE,
        )
    }

    /// Get data block by id
    pub fn get_data_block_id(&self, data_block_id: u32) -> u32 {
        self.data_area_start_block + data_block_id
    }

    /// Allocate a new inode
    pub fn alloc_inode(&mut self) -> u32 {
        self.inode_bitmap.alloc(&self.block_device).unwrap() as u32
    }

    #[allow(unused)]
    /// Deallocate a new inode (delete)
    pub fn dealloc_inode(&mut self) {
        unimplemented!()
    }

    /// Output block_id on device, not pos of bit in bitmap
    pub fn alloc_data(&mut self) -> u32 {
        self.data_area_start_block + self.data_bitmap.alloc(&self.block_device).unwrap() as u32
    }

    /// Input block_id on device, not pos of bit in bitmap
    pub fn dealloc_data(&mut self, block_id: u32) {
        get_block_cache(block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(0, |data_block: &mut DataBlock| data_block.fill(0));
        self.data_bitmap.dealloc(
            &self.block_device,
            (block_id - self.data_area_start_block) as usize,
        );
    }
}
