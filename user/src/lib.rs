#![no_std]
#![feature(linkage)]

#[macro_use]
pub mod console;
mod lang_item;
mod syscall;

extern "C" {
    fn start_bss();
    fn end_bss();
}

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    clear_bss();
    exit(main());
    panic!("unreachable after sys_exit!")
}

// "weak" symbol here coz "main" defined under src/bin/*.rs are actual "main"s
// to be applied
// if no "main" under src/bin/, our code compiles also but panic at runtime
#[no_mangle]
#[linkage = "weak"]
fn main() -> i32 {
    panic!("Cannot find main!")
}

fn clear_bss() {
    (start_bss as usize..end_bss as usize).for_each(|addr| unsafe {
        (addr as *mut u8).write_volatile(0);
    });
}

use syscall::{sys_exit, sys_write, sys_yield};

pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf)
}

pub fn exit(exit_code: i32) -> isize {
    sys_exit(exit_code)
}

pub fn yield_() -> isize {
    sys_yield()
}
