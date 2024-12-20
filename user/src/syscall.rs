use core::arch::asm;

use crate::{Dirent, SignalAction, Stat, TimeVal};

const SYSCALL_GETCWD: usize = 17;
const SYSCALL_DUP: usize = 24;
const SYSCALL_CONNECT: usize = 29;
const SYSCALL_LISTEN: usize = 30;
const SYSCALL_ACCEPT: usize = 31;
const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_LINKAT: usize = 37;
const SYSCALL_CHDIR: usize = 49;
const SYSCALL_OPENAT: usize = 56;
const SYSCALL_CLOSE: usize = 57;
const SYSCALL_PIPE: usize = 59;
const SYSCALL_GETDENTS: usize = 61;
const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_FSTAT: usize = 80;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_SLEEP: usize = 101;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_KILL: usize = 129;
const SYSCALL_SIGACTION: usize = 134;
const SYSCALL_SIGPROCMASK: usize = 135;
const SYSCALL_SIGRETURN: usize = 139;
const SYSCALL_GET_TIME: usize = 169;
const SYSCALL_GETPID: usize = 172;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_FORK: usize = 220;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_WAITPID: usize = 260;
const SYSCALL_THREAD_CREATE: usize = 1000;
const SYSCALL_GETTID: usize = 1001;
const SYSCALL_WAITTID: usize = 1002;
const SYSCALL_MUTEX_CREATE: usize = 1010;
const SYSCALL_MUTEX_LOCK: usize = 1011;
const SYSCALL_MUTEX_UNLOCK: usize = 1012;
const SYSCALL_SEMAPHORE_CREATE: usize = 1020;
const SYSCALL_SEMAPHORE_UP: usize = 1021;
const SYSCALL_SEMAPHORE_DOWN: usize = 1022;
const SYSCALL_CONDVAR_CREATE: usize = 1030;
const SYSCALL_CONDVAR_SIGNAL: usize = 1031;
const SYSCALL_CONDVAR_WAIT: usize = 1032;

fn syscall(id: usize, args: [usize; 3]) -> isize {
    let mut ret: isize;
    unsafe {
        asm!("ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x17") id
        );
    }
    ret
}

macro_rules! syscall {
    ($id:expr) => {
        syscall($id, [0, 0, 0])
    };
    ($id:expr, $a0:expr) => {
        syscall($id, [$a0, 0, 0])
    };
    ($id:expr, $a0:expr, $a1:expr) => {
        syscall($id, [$a0, $a1, 0])
    };
    ($id:expr, $a0:expr, $a1:expr, $a2:expr) => {
        syscall($id, [$a0, $a1, $a2])
    };
}

pub fn sys_openat(fd: isize, path: &str, flags: u32) -> isize {
    syscall!(
        SYSCALL_OPENAT,
        fd as usize,
        path.as_ptr() as usize,
        flags as usize
    )
}

pub fn sys_close(fd: usize) -> isize {
    syscall!(SYSCALL_CLOSE, fd)
}

pub fn sys_read(fd: usize, buf: &mut [u8]) -> isize {
    syscall!(SYSCALL_READ, fd, buf.as_mut_ptr() as usize, buf.len())
}

pub fn sys_write(fd: usize, buf: &[u8]) -> isize {
    syscall!(SYSCALL_WRITE, fd, buf.as_ptr() as usize, buf.len())
}

pub fn sys_exit(exit_code: i32) -> ! {
    syscall!(SYSCALL_EXIT, exit_code as usize);
    panic!("sys_exit never return!");
}

pub fn sys_yield() -> isize {
    syscall!(SYSCALL_YIELD)
}

pub fn sys_get_time(ts: &mut TimeVal) -> isize {
    syscall!(SYSCALL_GET_TIME, ts as *const _ as usize)
}

pub fn sys_mmap(
    start: usize,
    len: usize,
    prot: usize,
    flags: usize,
    fd: usize,
    offset: usize,
) -> isize {
    let packed_args = [prot, flags, fd, offset];
    syscall!(SYSCALL_MMAP, start, len, packed_args.as_ptr() as usize)
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    syscall!(SYSCALL_MUNMAP, start, len)
}

pub fn sys_getpid() -> isize {
    syscall!(SYSCALL_GETPID)
}

pub fn sys_fork() -> isize {
    syscall!(SYSCALL_FORK)
}

pub fn sys_exec(prog: &str, args: &[*const u8]) -> isize {
    syscall!(SYSCALL_EXEC, prog.as_ptr() as usize, args.as_ptr() as usize)
}

pub fn sys_waitpid(pid: isize, xstatus: &mut i32) -> isize {
    syscall!(SYSCALL_WAITPID, pid as usize, xstatus as *mut _ as usize)
}

pub fn sys_getcwd(path: &mut [u8]) -> isize {
    syscall!(SYSCALL_GETCWD, path.as_mut_ptr() as usize, path.len())
}

pub fn sys_mkdirat(fd: isize, path: &str) -> isize {
    syscall!(SYSCALL_MKDIRAT, fd as usize, path.as_ptr() as usize)
}

pub fn sys_chdir(path: &str) -> isize {
    syscall!(SYSCALL_CHDIR, path.as_ptr() as usize)
}

pub fn sys_unlinkat(fd: isize, path: &str) -> isize {
    syscall!(SYSCALL_UNLINKAT, fd as usize, path.as_ptr() as usize)
}

pub fn sys_linkat(fd: isize, oldpath: &str, newpath: &str) -> isize {
    syscall!(
        SYSCALL_LINKAT,
        fd as usize,
        oldpath.as_ptr() as usize,
        newpath.as_ptr() as usize
    )
}

pub fn sys_fstat(fd: usize, stat: &mut Stat) -> isize {
    syscall!(SYSCALL_FSTAT, fd as usize, stat as *mut _ as usize)
}

pub fn sys_pipe(pipe: &mut [usize]) -> isize {
    syscall!(SYSCALL_PIPE, pipe.as_mut_ptr() as usize)
}

pub fn sys_dup(fd: usize) -> isize {
    syscall!(SYSCALL_DUP, fd)
}

pub fn sys_kill(pid: usize, signum: i32) -> isize {
    syscall!(SYSCALL_KILL, pid, signum as usize)
}

pub fn sys_sigaction(
    signum: i32,
    action: *const SignalAction,
    old_action: *mut SignalAction,
) -> isize {
    syscall!(
        SYSCALL_SIGACTION,
        signum as usize,
        action as usize,
        old_action as usize
    )
}

pub fn sys_sigprocmask(mask: u32) -> isize {
    syscall!(SYSCALL_SIGPROCMASK, mask as usize)
}

pub fn sys_sigreturn() -> isize {
    syscall!(SYSCALL_SIGRETURN)
}

pub fn sys_getdents(fd: usize, entries: &mut [Dirent]) -> isize {
    syscall!(
        SYSCALL_GETDENTS,
        fd,
        entries.as_mut_ptr() as usize,
        entries.len()
    )
}

pub fn sys_thread_create(entry: usize, arg: usize) -> isize {
    syscall!(SYSCALL_THREAD_CREATE, entry, arg)
}

pub fn sys_gettid() -> isize {
    syscall!(SYSCALL_GETTID)
}

pub fn sys_waittid(tid: usize) -> isize {
    syscall!(SYSCALL_WAITTID, tid)
}

pub fn sys_sleep(ms: usize) -> isize {
    syscall!(SYSCALL_SLEEP, ms)
}

pub fn sys_mutex_create(blocking: bool) -> isize {
    let arg = if blocking { 1 } else { 0 };
    syscall!(SYSCALL_MUTEX_CREATE, arg)
}

pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    syscall!(SYSCALL_MUTEX_LOCK, mutex_id)
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    syscall!(SYSCALL_MUTEX_UNLOCK, mutex_id)
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    syscall!(SYSCALL_SEMAPHORE_CREATE, res_count)
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    syscall!(SYSCALL_SEMAPHORE_UP, sem_id)
}

pub fn sys_semaphore_down(sem_id: usize) -> isize {
    syscall!(SYSCALL_SEMAPHORE_DOWN, sem_id)
}

pub fn sys_condvar_create() -> isize {
    syscall!(SYSCALL_CONDVAR_CREATE)
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    syscall!(SYSCALL_CONDVAR_SIGNAL, condvar_id)
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    syscall!(SYSCALL_CONDVAR_WAIT, condvar_id, mutex_id)
}

pub fn sys_connect(dest: u32, sport: u16, dport: u16) -> isize {
    syscall!(
        SYSCALL_CONNECT,
        dest as usize,
        sport as usize,
        dport as usize
    )
}

// just listen for tcp connections now
pub fn sys_listen(sport: u16) -> isize {
    syscall!(SYSCALL_LISTEN, sport as usize)
}

pub fn sys_accept(socket_fd: usize) -> isize {
    syscall!(SYSCALL_ACCEPT, socket_fd)
}
