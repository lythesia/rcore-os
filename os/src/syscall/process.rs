use alloc::sync::Arc;

use crate::{fs, mm, task::*, timer};

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// get time
pub fn sys_get_time(ts: *mut TimeVal) -> isize {
    let us = timer::get_time_us();
    let dst_vs = mm::translated_byte_buffer(
        current_user_token(),
        ts as *const u8,
        core::mem::size_of::<TimeVal>(),
    );
    let ts = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let ts_ptr = (&ts as *const TimeVal) as *const u8;
    for (i, dst) in dst_vs.into_iter().enumerate() {
        let len = dst.len();
        unsafe {
            let src = core::slice::from_raw_parts(ts_ptr.wrapping_add(i * len), len);
            dst.copy_from_slice(src);
        }
    }
    0
}

pub fn sys_getpid() -> isize {
    let current_task = current_task().unwrap();
    current_task.getpid() as isize
}

pub fn sys_fork() -> isize {
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0; // a0
    add_task(new_task); // add new task to scheduler
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    let token = current_user_token();
    let path = mm::translated_str(token, path);
    if let Some(elf_inode) = fs::open_file(&path, fs::OpenFlags::RDONLY) {
        let elf_data = &elf_inode.read_all();
        let task = current_task().unwrap();
        task.exec(elf_data);
        // 这个返回值其实并没有意义, 因为我们在替换地址空间的时候本来就对 Trap 上下文重新进行了初始化
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let current_task = current_task().unwrap();
    let mut inner = current_task.inner_exclusive_access();

    // find arbitrary child (given `pid: -1`) OR child identified by `pid`
    let (idx, p) = match inner
        .children
        .iter()
        .enumerate()
        .find(|(_, p)| pid == -1 || p.getpid() == pid as usize)
    {
        Some(v) => v,
        _ => return -1,
    };

    // child not exited yet
    if !p.inner_exclusive_access().is_zombie() {
        return -2;
    }

    let p = inner.children.remove(idx);
    // now `p` is the only ref to (zombied) task
    assert_eq!(Arc::strong_count(&p), 1);
    let child_pid = p.getpid();
    let exit_code = p.inner_exclusive_access().exit_code;
    // set exit_code
    *mm::translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
    child_pid as isize
}

pub fn sys_halt() -> isize {
    println!("halt ...");
    crate::sbi::shutdown(false);
}
