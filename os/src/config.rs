//! Constants used in rCore

pub use crate::board::{CLOCK_FREQ, MEMORY_END, MMIO};

// Memory
pub const USER_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_HEAP_SIZE: usize = 0x30_0000;
pub const PAGE_SIZE: usize = 0x1000;
pub const PAGE_SIZE_BITS: usize = 12;
pub const TRAMPOLINE: usize = usize::MAX - PAGE_SIZE + 1;
pub const TRAP_CONTEXT: usize = TRAMPOLINE - PAGE_SIZE;

pub const MMAP_AREA_BASE: usize = 0x0000_0001_0000_0000; // base addr in user_space that nobody use
