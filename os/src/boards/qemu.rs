//! Constants used in rCore for qemu

pub const CLOCK_FREQ: usize = 12500000;
pub const MEMORY_END: usize = 0x8800_0000;

pub const MMIO: &[(usize, usize)] = &[
    (0x10001000, 0x1000), // Virtio Block in virt machine
];

pub type BlockDeviceImpl = crate::drivers::block::VirtIOBlock;
