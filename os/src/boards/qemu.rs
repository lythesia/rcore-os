//! Constants used in rCore for qemu

pub const CLOCK_FREQ: usize = 12500000;
pub const MEMORY_END: usize = 0x8800_0000;

pub const MMIO: &[(usize, usize)] = &[
    (0x0010_0000, 0x2000), // VIRT_TEST/RTC  in virt machine
    (0x0200_0000, 0x10000),
    (VIRT_PLIC, 0x210000), // VIRT_PLIC in virt machine
    (VIRT_UART, 0x9000),   // VIRT_UART0 with GPU  in virt machine
];

pub const VIRT_PLIC: usize = 0x0C00_0000;
pub const VIRT_UART: usize = 0x1000_0000;

pub type BlockDeviceImpl = crate::drivers::block::VirtIOBlock;
pub type CharDeviceImpl = crate::drivers::chardev::NS16550a<VIRT_UART>;

use crate::drivers::{
    plic::{IntrTargetPriority, PLIC},
    CharDevice, BLOCK_DEVICE, KEYBOARD_DEVICE, UART,
};

pub fn device_init() {
    use riscv::register::sie;
    let mut plic = unsafe { PLIC::new(VIRT_PLIC) };
    let hart_id: usize = 0;
    let supervisor = IntrTargetPriority::Supervisor;
    let machine = IntrTargetPriority::Machine;
    plic.set_threshold(hart_id, supervisor, 0);
    plic.set_threshold(hart_id, machine, 1);
    // irq nums: 5 keyboard, 6 mouse, 8 block, 10 uart
    for intr_src_id in [5, 8, 10] {
        plic.enable(hart_id, supervisor, intr_src_id);
        plic.set_priority(intr_src_id, 1);
    }
    unsafe {
        sie::set_sext();
    }
}

pub fn irq_handler() {
    let mut plic = unsafe { PLIC::new(VIRT_PLIC) };
    let intr_src_id = plic.claim(0, IntrTargetPriority::Supervisor);
    match intr_src_id {
        5 => KEYBOARD_DEVICE.handle_irq(),
        8 => BLOCK_DEVICE.handle_irq(),
        10 => UART.handle_irq(),
        _ => panic!("unsupported IRQ {}", intr_src_id),
    }
    plic.complete(0, IntrTargetPriority::Supervisor, intr_src_id);
}
