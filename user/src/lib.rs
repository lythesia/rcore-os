#![no_std]
#![feature(linkage)]
#![feature(alloc_error_handler)]

use alloc::vec::Vec;
use bitflags::bitflags;
use buddy_system_allocator::LockedHeap;
use syscall::*;

extern crate alloc;

#[macro_use]
pub mod console;
mod lang_item;
mod net;
pub use net::*;
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
pub extern "C" fn _start(argc: usize, argv: usize) -> ! {
    unsafe {
        HEAP.lock()
            .init(HEAP_SPACE.as_ptr() as usize, USER_HEAP_SIZE);
    }
    let mut v: Vec<&'static str> = Vec::new();
    for i in 0..argc {
        // read argv[i], which is ptr
        let str_start =
            unsafe { ((argv + i * core::mem::size_of::<usize>()) as *const usize).read_volatile() };
        // read len of argv[i] (just read until 0)
        let len = (0..)
            .find(|i| unsafe { ((str_start + *i) as *const u8).read_unaligned() == 0 })
            .unwrap();
        // append arg as str
        v.push(
            core::str::from_utf8(unsafe {
                // here we didn't plus '\0', but we know there IS one '\0' added by `sys_exec` at kernel side
                core::slice::from_raw_parts(str_start as *const u8, len)
            })
            .unwrap(),
        );
    }
    exit(main(argc, v.as_slice()))
}

// "weak" symbol here coz "main" defined under src/bin/*.rs are actual "main"s
// to be applied
// if no "main" under src/bin/, our code compiles also but panic at runtime
#[no_mangle]
#[linkage = "weak"]
fn main(_argc: usize, _argv: &[&str]) -> i32 {
    panic!("Cannot find main!")
}

// utils
#[macro_export]
macro_rules! vstore {
    ($var: expr, $value: expr) => {
        unsafe {
            core::ptr::write_volatile(core::ptr::addr_of_mut!($var), $value);
        }
    };
}

#[macro_export]
macro_rules! vload {
    ($var: expr) => {
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!($var)) }
    };
}

// syscall defs
bitflags! {
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RDRW = 1 << 1;
        const CREATE = 1 << 9;
        const TRUNC = 1 << 10;
    }
}

// fn assert_str_with_nul(s: &str) {
//     let last = unsafe {
//         let ptr = s.as_ptr().add(s.len());
//         *ptr
//     };
//     assert_eq!(last, 0)
// }

const AT_FDCWD: isize = -100;
pub fn open(path: &str, flags: OpenFlags) -> isize {
    sys_openat(AT_FDCWD, path, flags.bits)
}

pub fn openat(fd: usize, path: &str, flags: OpenFlags) -> isize {
    sys_openat(fd as isize, path, flags.bits)
}

pub fn close(fd: usize) -> isize {
    sys_close(fd)
}

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf)
}

pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf)
}

pub fn exit(exit_code: i32) -> ! {
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
    let ts = &mut TimeVal::new();
    match sys_get_time(ts) {
        0 => ((ts.sec & 0xffff) * 1000 + ts.usec / 1000) as isize,
        _ => -1,
    }
}

pub fn sleep(ms: usize) {
    let _ = sys_sleep(ms);
}

bitflags! {
    pub struct MMapFlags: u32 {
        const MAP_ANON = 0;
        const MAP_FILE = 1 << 0;
        const MAP_FIXED = 1 << 1;
    }
}

pub fn mmap(
    start: usize,
    len: usize,
    prot: usize,
    flags: MMapFlags,
    fd: usize,
    offset: usize,
) -> isize {
    sys_mmap(start, len, prot, flags.bits as usize, fd, offset)
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

pub fn exec(prog: &str, args: &[*const u8]) -> isize {
    sys_exec(prog, args)
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

pub fn waitpid_n(pid: usize, exit_code: &mut i32) -> isize {
    sys_waitpid(pid as isize, exit_code)
}

pub fn getcwd(path: &mut [u8]) -> isize {
    sys_getcwd(path)
}

pub fn mkdir(path: &str) -> isize {
    sys_mkdirat(AT_FDCWD, path)
}

pub fn mkdirat(fd: usize, path: &str) -> isize {
    sys_mkdirat(fd as isize, path)
}

pub fn chdir(path: &str) -> isize {
    sys_chdir(path)
}

pub fn unlink(path: &str) -> isize {
    sys_unlinkat(AT_FDCWD, path)
}

pub fn link(oldpath: &str, newpath: &str) -> isize {
    sys_linkat(AT_FDCWD, oldpath, newpath)
}

#[repr(C)]
#[derive(Default)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: StatMode,
    pub nlink: u32,
    pub size: u64,
    pad: [u64; 6],
}
impl Stat {
    pub fn new() -> Self {
        Self::default()
    }
}

bitflags! {
    #[derive(Default)]
    pub struct StatMode: u32 {
        const NULL  = 0;
        /// directory
        const DIR   = 0o040000;
        /// ordinary regular file
        const FILE  = 0o100000;
    }
}

pub fn fstat(fd: usize, stat: &mut Stat) -> isize {
    sys_fstat(fd, stat)
}

pub fn pipe(pipe_fd: &mut [usize]) -> isize {
    sys_pipe(pipe_fd)
}

pub fn dup(fd: usize) -> isize {
    sys_dup(fd)
}

pub fn kill(pid: usize, signum: i32) -> isize {
    sys_kill(pid, signum)
}

pub const SIGDEF: i32 = 0; // Default signal handling
pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
pub const SIGBUS: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGUSR1: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGUSR2: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGSTKFLT: i32 = 16;
pub const SIGCHLD: i32 = 17;
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;
pub const SIGTSTP: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;
pub const SIGURG: i32 = 23;
pub const SIGXCPU: i32 = 24;
pub const SIGXFSZ: i32 = 25;
pub const SIGVTALRM: i32 = 26;
pub const SIGPROF: i32 = 27;
pub const SIGWINCH: i32 = 28;
pub const SIGIO: i32 = 29;
pub const SIGPWR: i32 = 30;
pub const SIGSYS: i32 = 31;

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct SignalAction {
    pub handler: usize,
    pub mask: SignalFlags,
}

impl Default for SignalAction {
    fn default() -> Self {
        Self {
            handler: 0,
            mask: SignalFlags::empty(),
        }
    }
}

bitflags! {
    pub struct SignalFlags: i32 {
        const SIGDEF = 1; // Default signal handling
        const SIGHUP = 1 << 1;
        const SIGINT = 1 << 2;
        const SIGQUIT = 1 << 3;
        const SIGILL = 1 << 4;
        const SIGTRAP = 1 << 5;
        const SIGABRT = 1 << 6;
        const SIGBUS = 1 << 7;
        const SIGFPE = 1 << 8;
        const SIGKILL = 1 << 9;
        const SIGUSR1 = 1 << 10;
        const SIGSEGV = 1 << 11;
        const SIGUSR2 = 1 << 12;
        const SIGPIPE = 1 << 13;
        const SIGALRM = 1 << 14;
        const SIGTERM = 1 << 15;
        const SIGSTKFLT = 1 << 16;
        const SIGCHLD = 1 << 17;
        const SIGCONT = 1 << 18;
        const SIGSTOP = 1 << 19;
        const SIGTSTP = 1 << 20;
        const SIGTTIN = 1 << 21;
        const SIGTTOU = 1 << 22;
        const SIGURG = 1 << 23;
        const SIGXCPU = 1 << 24;
        const SIGXFSZ = 1 << 25;
        const SIGVTALRM = 1 << 26;
        const SIGPROF = 1 << 27;
        const SIGWINCH = 1 << 28;
        const SIGIO = 1 << 29;
        const SIGPWR = 1 << 30;
        const SIGSYS = 1 << 31;
    }
}

pub fn sigaction(
    signum: i32,
    action: Option<&SignalAction>,
    old_action: Option<&mut SignalAction>,
) -> isize {
    sys_sigaction(
        signum,
        action.map_or(core::ptr::null(), |a| a),
        old_action.map_or(core::ptr::null_mut(), |a| a),
    )
}

pub fn sigprocmask(mask: u32) -> isize {
    sys_sigprocmask(mask)
}

pub fn sigreturn() -> isize {
    sys_sigreturn()
}

/// The max length of inode name
const NAME_LENGTH_LIMIT: usize = 27;

#[repr(C, align(32))]
#[derive(Clone, Default)]
pub struct Dirent {
    pub ftype: FileType,
    pub name: [u8; NAME_LENGTH_LIMIT],
    pub next_offset: u32,
}

bitflags! {
    #[derive(Default)]
    pub struct FileType: u8 {
        const UNKNOWN = 0;
        const DIR = 1 << 0;
        const REG = 1 << 1;
    }
}

impl Dirent {
    pub fn name(&self) -> &str {
        let len = match self.name.iter().position(|v| v == &0) {
            Some(idx) => idx,
            _ => self.name.len(),
        };
        core::str::from_utf8(&self.name[..len]).unwrap()
    }
}

pub fn getdents(fd: usize, entries: &mut [Dirent]) -> isize {
    sys_getdents(fd, entries)
}

pub fn thread_create(entry: usize, arg: usize) -> isize {
    sys_thread_create(entry, arg)
}

pub fn gettid() -> isize {
    sys_gettid()
}

pub fn waittid(tid: usize) -> isize {
    loop {
        match sys_waittid(tid) {
            -2 => {
                yield_();
            }
            exit_code => return exit_code,
        }
    }
}

pub fn mutex_create() -> isize {
    sys_mutex_create(false)
}

pub fn mutex_blocking_create() -> isize {
    sys_mutex_create(true)
}

pub fn mutex_lock(mutex_id: usize) -> isize {
    sys_mutex_lock(mutex_id)
}

pub fn mutex_unlock(mutex_id: usize) -> isize {
    sys_mutex_unlock(mutex_id)
}

pub fn semaphore_create(res_count: usize) -> isize {
    sys_semaphore_create(res_count)
}

pub fn semaphore_acquire(sem_id: usize) -> isize {
    sys_semaphore_down(sem_id)
}

pub fn semaphore_release(sem_id: usize) -> isize {
    sys_semaphore_up(sem_id)
}

pub fn condvar_create() -> isize {
    sys_condvar_create()
}

pub fn condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    sys_condvar_wait(condvar_id, mutex_id)
}

pub fn condvar_signal(condvar_id: usize) -> isize {
    sys_condvar_signal(condvar_id)
}
