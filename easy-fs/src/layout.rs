use alloc::{sync::Arc, vec::Vec};

use crate::{block_cache::get_block_cache, block_dev::BlockDevice, BLOCK_SZ};

/// Magic number for sanity check
const EFS_MAGIC: u32 = 0x3b800001;
/// The max number of direct inodes
const INODE_DIRECT_COUNT: usize = 28;
/// The max length of inode name
const NAME_LENGTH_LIMIT: usize = 27;
/// The max number of indirect1 inodes
const INODE_INDIRECT1_COUNT: usize = BLOCK_SZ / 4;
/// The max number of indirect2 inodes
const INODE_INDIRECT2_COUNT: usize = INODE_INDIRECT1_COUNT * INODE_INDIRECT1_COUNT;
/// The upper bound of direct inode index
const DIRECT_BOUND: usize = INODE_DIRECT_COUNT;
/// The upper bound of indirect1 inode index
const INDIRECT1_BOUND: usize = DIRECT_BOUND + INODE_INDIRECT1_COUNT;

/// Super block (6*4 = 32B) of a filesystem
#[repr(C)]
pub struct SuperBlock {
    magic: u32,
    pub total_blocks: u32,
    pub inode_bitmap_blocks: u32,
    pub inode_area_blocks: u32,
    pub data_bitmap_blocks: u32,
    pub data_area_blocks: u32,
}

// just skip `magic`
impl core::fmt::Debug for SuperBlock {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SuperBlock")
            .field("total_blocks", &self.total_blocks)
            .field("inode_bitmap_blocks", &self.inode_bitmap_blocks)
            .field("inode_area_blocks", &self.inode_area_blocks)
            .field("data_bitmap_blocks", &self.data_bitmap_blocks)
            .field("data_area_blocks", &self.data_area_blocks)
            .finish()
    }
}

impl SuperBlock {
    pub fn initialize(
        &mut self,
        total_blocks: u32,
        inode_bitmap_blocks: u32,
        inode_area_blocks: u32,
        data_bitmap_blocks: u32,
        data_area_blocks: u32,
    ) {
        *self = Self {
            magic: EFS_MAGIC,
            total_blocks,
            inode_bitmap_blocks,
            inode_area_blocks,
            data_bitmap_blocks,
            data_area_blocks,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == EFS_MAGIC
    }
}

/// A indirect block
type IndirectBlock = [u32; BLOCK_SZ / 4];
/// A data block
type DataBlock = [u8; BLOCK_SZ];
/// A disk inode
#[repr(C)] // size == 32*u32 = 128B == 1/4 block
pub struct DiskInode {
    pub size: u32,
    // when file is small, `direct` refs 28 data blocks == 28*512 = 14KB
    pub direct: [u32; INODE_DIRECT_COUNT],
    // when file is large, `indirect1` refs to L1 index block, every u32 in it refs to
    // data block, so total 512/4*512 = 64KB
    pub indirect1: u32,
    // similar as `indrect1`, `indrect2` refs to L2 index block, so total 512/4*64KB = 8MB
    pub indirect2: u32,
    type_: DiskInodeType,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DiskInodeType {
    File,
    Directory,
}

impl DiskInode {
    pub fn initialize(&mut self, type_: DiskInodeType) {
        self.size = 0;
        self.direct.fill(0);
        self.indirect1 = 0;
        self.indirect2 = 0;
        self.type_ = type_;
    }

    pub fn initialize_dir<F: FnMut() -> u32>(
        &mut self,
        self_inode: u32,
        parent_inode: u32,
        mut data_alloc: F,
        block_device: &Arc<dyn BlockDevice>,
    ) {
        assert_eq!(self.type_, DiskInodeType::Directory);

        // increase size
        let file_count = (self.size as usize) / DIRENT_SZ; // should be 0 when create
        let new_size = ((file_count + 2) * DIRENT_SZ) as u32; // "." and ".."
        let blocks_needed = self.blocks_num_needed(new_size);
        let mut new_blocks = Vec::new();
        for _ in 0..blocks_needed {
            new_blocks.push(data_alloc());
        }
        self.increase_size(new_size, new_blocks, block_device);
        // write both dir entry points to self
        let buf = {
            let mut b = [0u8; 2 * DIRENT_SZ];
            let dir = DirEntry::new(".", self_inode);
            b[..DIRENT_SZ].copy_from_slice(dir.as_bytes());
            let dir = DirEntry::new("..", parent_inode);
            b[DIRENT_SZ..].copy_from_slice(dir.as_bytes());
            b
        };
        self.write_at(file_count * DIRENT_SZ, &buf[..], &block_device);
    }

    pub fn is_dir(&self) -> bool {
        self.type_ == DiskInodeType::Directory
    }

    pub fn is_file(&self) -> bool {
        self.type_ == DiskInodeType::File
    }

    pub fn get_block_id(&self, inner_id: u32, block_device: &Arc<dyn BlockDevice>) -> u32 {
        let inner_id = inner_id as usize;
        if inner_id < DIRECT_BOUND {
            self.direct[inner_id]
        } else if inner_id < INDIRECT1_BOUND {
            get_block_cache(self.indirect1 as usize, Arc::clone(block_device))
                .lock()
                .read(0, |indirect1: &IndirectBlock| {
                    indirect1[inner_id - INODE_DIRECT_COUNT]
                })
        } else {
            let last = inner_id - INDIRECT1_BOUND;
            // pos of indirect1 in indirect2
            let indirect1 = get_block_cache(self.indirect2 as usize, Arc::clone(block_device))
                .lock()
                .read(0, |indirect2: &IndirectBlock| {
                    indirect2[last / INODE_INDIRECT1_COUNT]
                });
            // pos in indirect2
            get_block_cache(indirect1 as usize, Arc::clone(block_device))
                .lock()
                .read(0, |indirect1: &IndirectBlock| {
                    indirect1[last % INODE_INDIRECT1_COUNT]
                })
        }
    }

    fn _data_blocks(size: u32) -> u32 {
        (size + BLOCK_SZ as u32 - 1) / BLOCK_SZ as u32
    }

    /// Return block number correspond to size.
    pub fn data_blocks(&self) -> u32 {
        Self::_data_blocks(self.size)
    }

    /// Return number of blocks needed include indirect1/2.
    pub fn total_blocks(size: u32) -> u32 {
        let data_blocks = Self::_data_blocks(size) as usize;
        let mut total = data_blocks as usize;
        // indirect1
        if data_blocks > DIRECT_BOUND {
            total += 1; // the direct1 block
        }
        // indirect2
        if data_blocks > INDIRECT1_BOUND {
            total += 1; // the direct2 block

            // how many direct1 ref by direct2
            // x = data_blocks - INDIRECT1_BOUND, blocks after indirect1 bound, those blocks ref by `indirect2`
            // then (x + T - 1) / T
            total +=
                (data_blocks - INDIRECT1_BOUND + INODE_INDIRECT1_COUNT - 1) / INODE_INDIRECT1_COUNT;
        }
        total as u32
    }

    /// How many more blocks need when extend data size to `new_size`
    pub fn blocks_num_needed(&self, new_size: u32) -> u32 {
        assert!(new_size >= self.size);
        Self::total_blocks(new_size) - Self::total_blocks(self.size)
    }

    /// Inncrease the size of current disk inode
    /// `new_blocks` layout:
    /// { dir blocks .. } | (block of `indir1`) { dir blocks ref by `indir1` } |
    /// (block of `indir2`)
    /// (block of `indir1_0` == `indir2[0]`) { dir blocks ref by `indir1_0` }
    /// (block of `indir1_1` == `indir2[1]`) { dir blocks ref by `indir1_1` }
    /// ..
    pub fn increase_size(
        &mut self,
        new_size: u32,
        new_blocks: Vec<u32>,
        block_device: &Arc<dyn BlockDevice>,
    ) {
        // to increase indeed
        assert!(new_size > self.size);
        // sufficient blocks provided
        assert!(new_blocks.len() >= self.blocks_num_needed(new_size) as usize);

        let mut curr_total = self.data_blocks() as usize;
        self.size = new_size;
        let mut new_total = self.data_blocks() as usize;

        let mut iter_new = new_blocks.into_iter();
        // if curr_total not beyond `DIRECT_BOUND`, fill `direct[last..]`
        while curr_total < INODE_DIRECT_COUNT.min(new_total) {
            self.direct[curr_total] = iter_new.next().unwrap();
            curr_total += 1;
        }

        // new_total > `INODE_DIRECT_COUNT`
        // alloc `indrect1`
        if new_total > INODE_DIRECT_COUNT {
            assert_eq!(curr_total, INODE_DIRECT_COUNT);
            self.indirect1 = iter_new.next().unwrap();
            // re-pos
            curr_total -= INODE_DIRECT_COUNT; // 0
            new_total -= INODE_DIRECT_COUNT;
        } else {
            return;
        }

        // fill blocks `indirect1` refs to
        get_block_cache(self.indirect1 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect1: &mut IndirectBlock| {
                while curr_total < INODE_INDIRECT1_COUNT.min(new_total) {
                    indirect1[curr_total] = iter_new.next().unwrap();
                    curr_total += 1;
                }
            });

        // new_total > `INODE_DIRECT1_COUNT`
        // alloc `indrect2`
        if new_total > INODE_INDIRECT1_COUNT {
            assert_eq!(curr_total, INODE_INDIRECT1_COUNT);
            self.indirect2 = iter_new.next().unwrap();
            // re-pos
            curr_total -= INODE_INDIRECT1_COUNT; // 0
            new_total -= INODE_INDIRECT1_COUNT;
        } else {
            return;
        }

        // fill blocks `indirect2`, including 2 levels
        let a1 = new_total / INODE_INDIRECT1_COUNT;
        let b1 = new_total % INODE_INDIRECT1_COUNT;
        get_block_cache(self.indirect2 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect2: &mut IndirectBlock| {
                while curr_total < new_total {
                    let a0 = curr_total / INODE_INDIRECT1_COUNT;
                    let b0 = curr_total % INODE_INDIRECT1_COUNT;
                    // alloc indirect2 entry
                    if b0 == 0 {
                        indirect2[a0] = iter_new.next().unwrap();
                    }
                    // fill blocks `indirect2[a0]` refs to
                    get_block_cache(indirect2[a0] as usize, Arc::clone(block_device))
                        .lock()
                        .modify(0, |indirect1: &mut IndirectBlock| {
                            let end = if a0 < a1 { INODE_INDIRECT1_COUNT } else { b1 };
                            for i in 0..end {
                                indirect1[i] = iter_new.next().unwrap();
                            }
                            curr_total += end;
                        });
                }
            })
    }

    /// Clear size to zero and return blocks that should be deallocated.
    /// We will clear the block contents to zero later.
    pub fn clear_size(&mut self, block_device: &Arc<dyn BlockDevice>) -> Vec<u32> {
        let mut v = Vec::new();
        let mut data_blocks = self.data_blocks() as usize;
        self.size = 0;
        let mut current_blocks = 0usize;
        // direct
        while current_blocks < data_blocks.min(INODE_DIRECT_COUNT) {
            v.push(self.direct[current_blocks]);
            self.direct[current_blocks] = 0;
            current_blocks += 1;
        }
        // indirect1 block
        if data_blocks > INODE_DIRECT_COUNT {
            v.push(self.indirect1);
            data_blocks -= INODE_DIRECT_COUNT;
            current_blocks = 0;
        } else {
            return v;
        }
        // indirect1
        get_block_cache(self.indirect1 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect1: &mut IndirectBlock| {
                while current_blocks < data_blocks.min(INODE_INDIRECT1_COUNT) {
                    v.push(indirect1[current_blocks]);
                    current_blocks += 1;
                }
            });
        self.indirect1 = 0;
        // indirect2 block
        if data_blocks > INODE_INDIRECT1_COUNT {
            v.push(self.indirect2);
            data_blocks -= INODE_INDIRECT1_COUNT;
        } else {
            return v;
        }
        // indirect2
        assert!(data_blocks <= INODE_INDIRECT2_COUNT);
        let a1 = data_blocks / INODE_INDIRECT1_COUNT;
        let b1 = data_blocks % INODE_INDIRECT1_COUNT;
        get_block_cache(self.indirect2 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect2: &mut IndirectBlock| {
                // full indirect1 blocks
                for entry in indirect2.iter_mut().take(a1) {
                    v.push(*entry);
                    get_block_cache(*entry as usize, Arc::clone(block_device))
                        .lock()
                        .modify(0, |indirect1: &mut IndirectBlock| {
                            for entry in indirect1.iter() {
                                v.push(*entry);
                            }
                        });
                }
                // last indirect1 block
                if b1 > 0 {
                    v.push(indirect2[a1]);
                    get_block_cache(indirect2[a1] as usize, Arc::clone(block_device))
                        .lock()
                        .modify(0, |indirect1: &mut IndirectBlock| {
                            for entry in indirect1.iter().take(b1) {
                                v.push(*entry);
                            }
                        });
                }
            });
        self.indirect2 = 0;
        v
    }

    /// Read data from current disk inode
    pub fn read_at(
        &self,
        offset: usize,
        buf: &mut [u8],
        block_device: &Arc<dyn BlockDevice>,
    ) -> usize {
        let mut start = offset;
        let end = (offset + buf.len()).min(self.size as usize);
        if start >= end {
            return 0;
        }

        let mut start_block = start / BLOCK_SZ;
        let mut read_size = 0;
        // need to tell [start_block, end_block) at direct? indirect1? indirect2?
        loop {
            let end_of_current_block = ((start / BLOCK_SZ + 1) * BLOCK_SZ).min(end);
            let block_read_size = end_of_current_block - start;
            let dst = &mut buf[read_size..read_size + block_read_size];
            get_block_cache(
                self.get_block_id(start_block as u32, block_device) as usize,
                Arc::clone(block_device),
            )
            .lock()
            .read(0, |data_block: &DataBlock| {
                let src = &data_block[start % BLOCK_SZ..start % BLOCK_SZ + block_read_size];
                dst.copy_from_slice(src);
            });
            read_size += block_read_size;
            // to next block
            if end_of_current_block == end {
                break;
            }
            start_block += 1;
            start = end_of_current_block;
        }
        read_size
    }

    /// Write data into current disk inode
    /// size must be adjusted properly beforehand
    pub fn write_at(
        &mut self,
        offset: usize,
        buf: &[u8],
        block_device: &Arc<dyn BlockDevice>,
    ) -> usize {
        let mut start = offset;
        let end = (offset + buf.len()).min(self.size as usize);
        assert!(start <= end);
        let mut start_block = start / BLOCK_SZ;
        let mut write_size = 0;

        loop {
            let end_of_current_block = ((start / BLOCK_SZ + 1) * BLOCK_SZ).min(end);
            let block_write_size = end_of_current_block - start;
            let src = &buf[write_size..write_size + block_write_size];
            get_block_cache(
                self.get_block_id(start_block as u32, block_device) as usize,
                Arc::clone(block_device),
            )
            .lock()
            .modify(0, |data_block: &mut DataBlock| {
                let dst = &mut data_block[start % BLOCK_SZ..start % BLOCK_SZ + block_write_size];
                dst.copy_from_slice(src);
            });
            write_size += block_write_size;
            // to next block
            if end_of_current_block == end {
                break;
            }
            start_block += 1;
            start = end_of_current_block;
        }
        write_size
    }
}

/// A directory entry
#[repr(C)]
pub struct DirEntry {
    name: [u8; NAME_LENGTH_LIMIT + 1], // filename len at most 27 + '\0'
    inode_number: u32,
}
/// Size of a directory entry
/// total 512/32 == 16 entries per block
pub const DIRENT_SZ: usize = 32;

impl DirEntry {
    pub fn new_empty() -> Self {
        Self {
            name: [0; NAME_LENGTH_LIMIT + 1],
            inode_number: 0,
        }
    }

    pub fn new(name: &str, inode_number: u32) -> Self {
        assert!(name.len() <= NAME_LENGTH_LIMIT);
        let mut buf = [0; NAME_LENGTH_LIMIT + 1];
        buf[..name.len()].copy_from_slice(name.as_bytes());
        Self {
            name: buf,
            inode_number,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self as *const _ as usize as *const u8, DIRENT_SZ) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self as *mut _ as usize as *mut u8, DIRENT_SZ) }
    }

    pub fn name(&self) -> &str {
        let len = self
            .name
            .iter()
            .position(|c| c == &0)
            .expect("name not end with \\0!");
        core::str::from_utf8(&self.name[..len]).unwrap()
    }
    pub fn inode_number(&self) -> u32 {
        self.inode_number
    }
}
