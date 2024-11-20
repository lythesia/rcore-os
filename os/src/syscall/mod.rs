mod fs;
mod mem;
mod process;

use fs::*;
use mem::*;
use process::*;

const SYSCALL_GETCWD: usize = 17;
const SYSCALL_DUP: usize = 24;
const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_LINKAT: usize = 37;
const SYSCALL_CHDIR: usize = 49;
const SYSCALL_OPENAT: usize = 56;
const SYSCALL_CLOSE: usize = 57;
const SYSCALL_PIPE: usize = 59;
const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_FSTAT: usize = 80;
const SYSCALL_EXIT: usize = 93;
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
const SYSCALL_HALT: usize = 555;

macro_rules! bail_exit {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(exit) => return exit,
        }
    };
}
pub(crate) use bail_exit;

pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    match syscall_id {
        SYSCALL_GETCWD => sys_getcwd(args[0] as *mut u8, args[1]),
        SYSCALL_DUP => sys_dup(args[0]),
        SYSCALL_MKDIRAT => sys_mkdirat(args[0] as isize, args[1] as *const u8),
        SYSCALL_UNLINKAT => sys_unlinkat(args[0] as isize, args[1] as *const u8),
        SYSCALL_LINKAT => sys_linkat(args[0] as isize, args[1] as *const u8, args[2] as *const u8),
        SYSCALL_CHDIR => sys_chdir(args[0] as *const u8),
        SYSCALL_OPENAT => sys_openat(args[0] as isize, args[1] as *const u8, args[2] as u32),
        SYSCALL_CLOSE => sys_close(args[0]),
        SYSCALL_PIPE => sys_pipe(args[0] as *mut usize),
        SYSCALL_READ => sys_read(args[0], args[1] as *const u8, args[2]),
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_FSTAT => sys_fstat(args[0] as usize, args[1] as *mut Stat),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_KILL => sys_kill(args[0], args[1] as i32),
        SYSCALL_SIGACTION => sys_sigaction(args[0] as i32, args[1] as *const _, args[2] as *mut _),
        SYSCALL_SIGPROCMASK => sys_sigprocmask(args[0] as u32),
        SYSCALL_SIGRETURN => sys_sigreturn(),
        SYSCALL_GET_TIME => sys_get_time(args[0] as *mut TimeVal),
        SYSCALL_GETPID => sys_getpid(),
        SYSCALL_MUNMAP => sys_munmap(args[0], args[1]),
        SYSCALL_FORK => sys_fork(),
        SYSCALL_EXEC => sys_exec(args[0] as *const u8, args[1] as *const usize),
        SYSCALL_MMAP => {
            let (start, len) = (args[0], args[1]);
            let [prot, flags, fd, offset] = unpack_args(args[2] as *const usize);
            sys_mmap(start, len, prot, flags, fd, offset)
        }
        SYSCALL_WAITPID => sys_waitpid(args[0] as isize, args[1] as *mut i32),
        SYSCALL_HALT => sys_halt(),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}

fn unpack_args<const N: usize>(args_ptr: *const usize) -> [usize; N] {
    let total: usize = N * core::mem::size_of::<usize>();

    let token = crate::task::current_user_token();
    let mut ret = [0; N];

    // turn [usize] into [u8]
    let ptr = args_ptr as *const u8;
    let vs = crate::mm::translated_byte_buffer(token, ptr, total);

    for (i, slice) in vs.into_iter().enumerate() {
        let len = slice.len();
        let dst_ptr = ret.as_mut_ptr() as *mut u8;
        let dst = unsafe { core::slice::from_raw_parts_mut(dst_ptr.wrapping_add(i * len), len) };
        dst.copy_from_slice(slice);
    }

    ret
}
