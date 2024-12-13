#![no_main]
#![no_std]
#![feature(array_windows)]
#![feature(alloc_error_handler)]
#![feature(slice_split_once)]
#![feature(trait_upcasting)]

extern crate alloc;
extern crate bitflags;

#[path = "boards/qemu.rs"]
mod board;
mod cast;
mod config;
#[macro_use]
mod console;
mod drivers;
mod fs;
mod lang_item;
mod logging;
mod mm;
mod net;
mod sbi;
mod sync;
mod syscall;
mod task;
mod timer;
mod trap;

use core::arch::global_asm;
use drivers::{CharDevice, KEYBOARD_DEVICE, UART};
use lazy_static::lazy_static;
use sync::UPIntrFreeCell;
global_asm!(include_str!("entry.asm"));

lazy_static! {
    pub static ref DEV_NON_BLOCKING_ACCESS: UPIntrFreeCell<bool> =
        unsafe { UPIntrFreeCell::new(false) };
}

#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();

    mm::init();
    UART.init();
    println!("KERN: init keyboard");
    let _keyboard = KEYBOARD_DEVICE.clone();
    println!("KERN: init trap");
    trap::init();
    trap::enable_timer_interrupt();
    timer::set_next_trigger();

    board::device_init();

    task::add_initproc();
    *DEV_NON_BLOCKING_ACCESS.exclusive_access() = true;

    logging::init();

    task::run_tasks();
    panic!("Unreachable in rust_main!");
}

unsafe extern "C" {
    fn sbss();
    fn ebss();
}

fn clear_bss() {
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}
