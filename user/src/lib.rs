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

use syscall::{sys_exit, sys_get_time, sys_task_info, sys_write, sys_yield};

pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf)
}

pub fn exit(exit_code: i32) -> isize {
    sys_exit(exit_code)
}

pub fn yield_() -> isize {
    sys_yield()
}

pub fn get_time() -> isize {
    sys_get_time()
}

pub fn sleep(ms: usize) {
    let start = get_time();
    while get_time() < start + ms as isize {
        sys_yield();
    }
}

// copy from kernel src
#[derive(Clone, Copy, PartialEq)]
pub enum TaskStatus {
    UnInit,
    Ready,
    Running,
    Exited,
}

const MAX_SYSCALL_NUM: usize = 500;
#[repr(C)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    pub status: TaskStatus,
    /// The numbers of syscall called by task
    pub syscall_times: [usize; MAX_SYSCALL_NUM],
    /// Total running time of task, which consists of kernel time and user time
    pub time: usize,
}

impl TaskInfo {
    pub fn new() -> Self {
        Self {
            status: TaskStatus::UnInit,
            syscall_times: [0; MAX_SYSCALL_NUM],
            time: 0,
        }
    }
}

pub fn task_info(ti: &TaskInfo) -> isize {
    sys_task_info(ti)
}
