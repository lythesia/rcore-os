use alloc::{sync::Arc, vec::Vec};

use crate::{
    fs,
    mm::{self, translate_ref, translated_str},
    task::*,
    timer,
};

use super::bail_exit;

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
    let curr_task = current_task().unwrap();
    curr_task.getpid() as isize
}

pub fn sys_fork() -> isize {
    let curr_task = current_task().unwrap();
    let new_task = curr_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0; // a0
    add_task(new_task); // add new task to scheduler
    new_pid as isize
}

pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
    let token = current_user_token();
    let path = mm::translated_str(token, path);
    let mut args_vec = Vec::new();
    loop {
        let arg_str_ptr = *mm::translate_ref(token, args);
        if arg_str_ptr == 0 {
            break;
        }
        args_vec.push(translated_str(token, arg_str_ptr as *const u8));
        unsafe {
            args = args.add(1);
        }
    }
    if let Some(elf_inode) = fs::open_file(&path, fs::OpenFlags::RDONLY) {
        let elf_data = &elf_inode.read_all();
        let task = current_task().unwrap();
        let argc = args_vec.len();
        task.exec(elf_data, args_vec);
        // !!return argc because cx.x[10] will be covered with it later
        argc as isize
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let curr_task = current_task().unwrap();
    let mut inner = curr_task.inner_exclusive_access();

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

pub fn sys_kill(pid: usize, signum: i32) -> isize {
    let flag = bail_exit!(SignalFlags::from_bits(1 << signum).ok_or(-1));
    let task = bail_exit!(pid2task(pid).ok_or(-1));
    let mut inner = task.inner_exclusive_access();
    if inner.signals.contains(flag) {
        return -1;
    }
    inner.signals.insert(flag);
    0
}

pub fn sys_sigprocmask(mask: u32) -> isize {
    let new_mask = bail_exit!(SignalFlags::from_bits(mask).ok_or(-1));
    let task = bail_exit!(current_task().ok_or(-1));
    let mut inner = task.inner_exclusive_access();
    let old_mask = inner.signal_mask;
    inner.signal_mask = new_mask;
    old_mask.bits() as isize
}

fn check_sigaction_error(signal: SignalFlags, action: usize, old_action: usize) -> bool {
    action == 0
        || old_action == 0
        || signal == SignalFlags::SIGKILL
        || signal == SignalFlags::SIGSTOP
}

pub fn sys_sigaction(
    signum: i32,
    action: *const SignalAction,
    old_action: *mut SignalAction,
) -> isize {
    if signum as usize > MAX_SIG {
        return -1;
    }

    let flag = bail_exit!(SignalFlags::from_bits(1 << signum).ok_or(-1));
    if check_sigaction_error(flag, action as usize, old_action as usize) {
        return -1;
    }

    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    let token = inner.get_user_token();

    let prev_action = inner.signal_actions.table[signum as usize];
    *mm::translated_refmut(token, old_action) = prev_action;
    inner.signal_actions.table[signum as usize] = *translate_ref(token, action);
    0
}

pub fn sys_sigreturn() -> isize {
    let task = bail_exit!(current_task().ok_or(-1));
    let mut inner = task.inner_exclusive_access();
    inner.handling_sig = -1;
    // restore trap_cx
    let trap_cx = inner.get_trap_cx();
    *trap_cx = inner.trap_cx_backup.take().unwrap();
    trap_cx.x[10] as isize
}
