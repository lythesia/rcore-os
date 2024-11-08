#![no_std]
#![feature(linkage)]

#[macro_use]
pub mod console;
mod lang_item;
pub mod syscall;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
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

use syscall::*;

pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf)
}

pub fn exit(exit_code: i32) -> isize {
    sys_exit(exit_code)
}

pub fn yield_() -> isize {
    sys_yield()
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}
impl TimeVal {
    pub fn new() -> Self {
        Self::default()
    }
}

pub fn get_time() -> isize {
    let ts = &TimeVal::new();
    match sys_get_time(ts) {
        0 => ((ts.sec & 0xffff) * 1000 + ts.usec / 1000) as isize,
        _ => -1,
    }
}

pub fn sleep(ms: usize) {
    let start = get_time();
    while get_time() < start + ms as isize {
        sys_yield();
    }
}

pub fn mmap(start: usize, len: usize, prot: usize) -> isize {
    sys_mmap(start, len, prot)
}

pub fn munmap(start: usize, len: usize) -> isize {
    sys_munmap(start, len)
}
