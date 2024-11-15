use alloc::sync::Arc;

use crate::{block_cache::get_block_cache, block_dev::BlockDevice, BLOCK_SZ};

/// A bitmap block
type BitmapBlock = [u64; 64];

/// Number of bits in a block
const BLOCK_BITS: usize = BLOCK_SZ * 8;

pub struct Bitmap {
    start_block_id: usize,
    // how many blocks to hold bitmap
    blocks: usize,
}

impl Bitmap {
    pub fn new(start_block_id: usize, blocks: usize) -> Self {
        Self {
            start_block_id,
            blocks,
        }
    }

    /// Output pos of bit in this bitmap
    pub fn alloc(&mut self, block_device: &Arc<dyn BlockDevice>) -> Option<usize> {
        for idx in 0..self.blocks {
            // 1. locate no.(start + idx) block
            let pos = get_block_cache(idx + self.start_block_id, Arc::clone(block_device))
                .lock()
                // 2. cast block[0..] as [u64;64] (512bytes == 1 block)
                .modify(0, |bitmap_block: &mut BitmapBlock| {
                    if let Some((bits64_pos, inner_pos)) = bitmap_block
                        .iter()
                        .enumerate()
                        // 3. find one u64 that not all bits been allocated already
                        .find(|(_, bits64)| **bits64 != u64::MAX)
                        // 4. trailing_ones gives how many(n) 1's in 0bxx11..11, so (1<<n) is the bit(not allocated) we're searching
                        .map(|(bits64_pos, bits64)| (bits64_pos, bits64.trailing_ones() as usize))
                    {
                        // modify cache
                        // 5. update this u64 with (1<<n) (in 4)
                        bitmap_block[bits64_pos] |= 1u64 << inner_pos;
                        // 6. return postion of this bit among all bits
                        Some(idx * BLOCK_BITS + bits64_pos * 64 + inner_pos)
                    } else {
                        None
                    }
                });
            if pos.is_some() {
                return pos;
            }
        }
        None
    }

    /// Input pos of bit in this bitmap
    pub fn dealloc(&self, block_device: &Arc<dyn BlockDevice>, bit: usize) {
        let (block_pos, bits64_pos, inner_pos) = decompsition(bit);
        get_block_cache(block_pos + self.start_block_id, Arc::clone(block_device))
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                assert!(bitmap_block[bits64_pos] & (1u64 << inner_pos) > 0);
                bitmap_block[bits64_pos] &= !(1u64 << inner_pos);
            })
    }

    pub fn maxmium(&self) -> usize {
        self.blocks * BLOCK_BITS
    }
}

/// Return (block_pos, bits64_pos, inner_pos)
fn decompsition(mut bit: usize) -> (usize, usize, usize) {
    let block_pos = bit / BLOCK_BITS;
    bit %= BLOCK_BITS;
    (block_pos, bit / 64, bit % 64)
}
