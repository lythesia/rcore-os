#![no_main]
#![no_std]
#![feature(array_windows)]
#![feature(alloc_error_handler)]
#![feature(slice_split_once)]

extern crate alloc;
extern crate bitflags;

#[path = "boards/qemu.rs"]
mod board;
mod config;
#[macro_use]
mod console;
mod lang_item;
mod loader;
mod logging;
mod mm;
mod sbi;
mod sync;
mod syscall;
mod task;
#[allow(unused)]
mod timer;
mod trap;

use core::arch::global_asm;
global_asm!(include_str!("entry.asm"));
global_asm!(include_str!("link_app.S"));

extern "C" {
    fn stext();
    fn etext();

    fn srodata();
    fn erodata();

    fn sdata();
    fn edata();

    fn boot_stack_top();
    fn boot_stack_lower_bound();

    fn sbss();
    fn ebss();

    fn ekernel();
}

#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    logging::init();

    log::info!(
        "[kernel] {:<10} [{:#x}, {:#x})",
        ".text",
        stext as usize,
        etext as usize
    );
    log::info!(
        "[kernel] {:<10} [{:#x}, {:#x})",
        ".rodata",
        srodata as usize,
        erodata as usize
    );
    log::info!(
        "[kernel] {:<10} [{:#x}, {:#x})",
        ".data",
        sdata as usize,
        edata as usize
    );
    log::info!(
        "[kernel] {:<10} [{:#x}, {:#x})",
        "boot_stack",
        boot_stack_lower_bound as usize,
        boot_stack_top as usize
    );
    log::info!(
        "[kernel] {:<10} [{:#x}, {:#x})",
        ".bss",
        sbss as usize,
        ebss as usize
    );
    log::info!(
        "[kernel] {:<10} [{:#x}, {:#x})",
        "phys",
        ekernel as usize,
        config::MEMORY_END,
    );

    mm::init();
    mm::remap_test();

    task::add_initproc();

    trap::init();
    trap::enable_timer_interrupt();
    timer::set_next_trigger();

    loader::list_apps();
    task::run_tasks();
    panic!("Unreachable in rust_main!");
}

fn clear_bss() {
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}
