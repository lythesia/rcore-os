#![no_std]
#![feature(linkage)]
#![feature(alloc_error_handler)]

use buddy_system_allocator::LockedHeap;
use syscall::*;

#[macro_use]
pub mod console;
mod lang_item;
pub mod syscall;

const USER_HEAP_SIZE: usize = 0x4000; // 16K

// locate at .bss
static mut HEAP_SPACE: [u8; USER_HEAP_SIZE] = [0; USER_HEAP_SIZE];

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

#[alloc_error_handler]
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    unsafe {
        HEAP.lock()
            .init(HEAP_SPACE.as_ptr() as usize, USER_HEAP_SIZE);
    }
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

// syscall defs

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf)
}

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

pub fn getpid() -> isize {
    sys_getpid()
}

pub fn fork() -> isize {
    sys_fork()
}

pub fn exec(prog: &str) -> isize {
    sys_exec(prog)
}

pub fn wait(exit_code: &mut i32) -> isize {
    loop {
        match sys_waitpid(-1, exit_code) {
            -2 => {
                sys_yield();
            }
            n => return n,
        }
    }
}

pub fn waitpid(pid: usize, exit_code: &mut i32) -> isize {
    loop {
        match sys_waitpid(pid as isize, exit_code) {
            -2 => {
                sys_yield();
            }
            n => return n,
        }
    }
}

pub fn halt() -> isize {
    sys_halt()
}

pub fn spawn(prog: &str) -> isize {
    sys_spawn(prog)
}
