use alloc::{sync::Arc, vec::Vec};
pub use context::TaskContext;
use id::TaskUserRes;
use lazy_static::lazy_static;
use manager::remove_from_pid2process;

use crate::fs;

mod action;
mod context;
mod id;
mod manager;
mod mem;
mod process;
mod processor;
mod signal;
mod switch;
mod task;

pub use action::*;
pub use manager::{add_task, pid2process, wakeup_task};
pub use mem::*;
pub use process::{FileMapping, MMapReserve, MapRange, ProcessControlBlock};
pub use processor::{
    current_kstack_top, current_process, current_task, current_trap_cx, current_trap_cx_user_va,
    current_user_token, run_tasks, schedule, user_time_end, user_time_start,
};
pub use signal::{SignalFlags, MAX_SIG};
pub use task::{TaskControlBlock, TaskStatus};

lazy_static! {
    pub static ref INITPROC: Arc<ProcessControlBlock> = {
        let inode = fs::open_file("initproc", fs::OpenFlags::RDONLY).unwrap();
        let elf = inode.read_all();
        ProcessControlBlock::new(&elf)
    };
}

pub fn suspend_current_and_run_next() {
    // There must be an application running.
    let task = processor::take_current_task().unwrap();
    // 当仅有一个任务的时候, suspend_current_and_run_next 的效果是会继续执行这个任务
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // {
    //     let process = task.process.upgrade().unwrap();
    //     process.inner_exclusive_access().kernel_time += processor::refresh_stop_watch();
    // }
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);

    add_task(task); // enqueue to manager
    processor::schedule(task_cx_ptr);
}

/// This function must be followed by a schedule
pub fn block_current_task() -> *mut TaskContext {
    let task = processor::take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    // {
    //     let process = task.process.upgrade().unwrap();
    //     println!("@block_current_task process.inner_exclusive_access");
    //     process.inner_exclusive_access().kernel_time += processor::refresh_stop_watch();
    //     println!("@block_current_task done process.inner_exclusive_access");
    // }
    task_inner.task_status = TaskStatus::Blocked;
    &mut task_inner.task_cx as *mut TaskContext
}

pub fn block_current_and_run_next() {
    let task_cx_ptr = block_current_task();
    processor::schedule(task_cx_ptr);
}

pub fn exit_current_and_run_next(exit_code: i32) {
    // There must be an application running.
    let task = processor::take_current_task().unwrap();
    let process = task.process.upgrade().unwrap();

    let mut task_inner = task.inner_exclusive_access();
    let tid = task_inner.res.as_ref().unwrap().tid;
    // set exit_code
    task_inner.exit_code = Some(exit_code);
    // dealloc user res
    task_inner.res = None;
    drop(task_inner);
    drop(task);
    // terminate process if it's main thread
    if tid == 0 {
        let pid = process.getpid();
        if pid == id::IDLE_PID {
            println!(
                "[kernel] Idle process exit with exit_code {} ...",
                exit_code
            );
            if exit_code != 0 {
                crate::sbi::shutdown(true)
            } else {
                crate::sbi::shutdown(false)
            }
        }
        // must remove from pid2task, else sys_wait will see this task ref_count not 1
        remove_from_pid2process(pid);
        let mut process_inner = process.inner_exclusive_access();
        // curr_task完成
        // process_inner.kernel_time += processor::refresh_stop_watch();
        process_inner.is_zombie = true;
        process_inner.exit_code = exit_code;
        // access initproc TCB exclusively
        {
            let mut initproc_inner = INITPROC.inner_exclusive_access();
            for child in process_inner.children.iter() {
                child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
                initproc_inner.children.push(child.clone());
            }
        }
        // dealloc user res of all threads
        let mut recycle_res = Vec::<TaskUserRes>::new();
        for task in process_inner.tasks.iter().filter(|t| t.is_some()) {
            let task = task.as_ref().unwrap();
            // remove_inactive_task(task.clone());
            let mut task_inner = task.inner_exclusive_access();
            if let Some(res) = task_inner.res.take() {
                recycle_res.push(res);
            }
        }
        // dealloc_tid and dealloc_user_res require access to PCB inner, so we
        // need to collect those user res first, then release process_inner
        // for now to avoid deadlock/double borrow problem.
        drop(process_inner);
        recycle_res.clear();

        let mut process_inner = process.inner_exclusive_access();
        process_inner.children.clear();
        // deallocate user space
        process_inner.memory_set.recycle_data_pages();
        // drop fd's
        process_inner.fd_table.clear();
        // write back dirty pages
        for mapping in &process_inner.file_mappings {
            mapping.sync();
        }
        // recycle all threads except main thread, coz it's currently executing!
        // see: https://github.com/rcore-os/rCore-Tutorial-Book-v3/issues/136#issuecomment-1955838457
        while process_inner.tasks.len() > 1 {
            process_inner.tasks.pop();
        }
    }
    drop(process);
    // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    // _unused只是一个placeholder, 代码上看这里调用了__switch(_unused, idle), 即保存当前的task_cx到_unused并切换的idle_task
    // idle_task其实是Processor的run_tasks的loop; 但实际上当前task已经从Processor上被移除(take)了, 当然它也不存在于TaskManager,
    // 因为它本来就是从TaskManager上fetch到Processor上的, 所以这个保存的task_cx永远不可能被switch回来, 故不care
    // Q: 如果exit的是唯一的一个task会怎样?
    // A: run_tasks会一直(在S态)loop, 且因为在S态的关系, 此时时钟中断不会被响应(处理), 因此suspend_current_and_run_next不会被
    // 调用, 也就不会触发take_current_task().unwrap()的panic
    processor::schedule(&mut _unused as *mut TaskContext);
}

pub fn add_initproc() {
    let _init = INITPROC.clone();
}

pub fn current_add_signal(signal: SignalFlags) {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    inner.signals |= signal;
}

pub fn current_handle_signals() {
    loop {
        check_pending_signals();
        let (frozen, killed) = {
            let task = current_task().unwrap();
            let inner = task.inner_exclusive_access();
            (inner.signal_processor.frozen, inner.signal_processor.killed)
        };
        // abort signal handling when:
        // 1. current process got SIGCONT (OR it's never been SIGSTOP-ed), which means this
        // task done handling pending signals and can move on
        // 2. current process got SIGKILL and this task be killed
        if !frozen || killed {
            break;
        }
        // frozen && !killed, then we give other process chance to fire SIGCONT to self
        // and when this task is scheduled again, `frozen` will still be true, thus "yield"
        // again (via `suspend_current_and_run_next`)
        suspend_current_and_run_next();
    }
}

fn check_pending_signals() {
    for signum in 0..=MAX_SIG {
        let signal = SignalFlags::from_bits_truncate(1 << signum);
        // we access current task inside loop to shorten UPSafeCell section
        let task = current_task().unwrap();
        let signals = {
            let process = task.process.upgrade().unwrap();
            let inner = process.inner_exclusive_access();
            inner.signals
        };
        // signum not fired
        if !signals.contains(signal) {
            continue;
        }
        let inner = task.inner_exclusive_access();
        // masked by task
        if inner.signal_processor.is_global_masked(signal) {
            continue;
        }
        // masked by handling signal action
        if inner.signal_processor.is_handling_masked(signal) {
            continue;
        }
        drop(inner);
        drop(task);
        if matches!(
            signal,
            SignalFlags::SIGKILL
                | SignalFlags::SIGSTOP
                | SignalFlags::SIGCONT
                | SignalFlags::SIGDEF
        ) {
            // kernel signal
            call_kernel_signal_handler(signal);
        } else {
            // user signal
            call_user_signal_handler(signum, signal);
            return;
        }
    }
}

fn call_kernel_signal_handler(signal: SignalFlags) {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    match signal {
        SignalFlags::SIGSTOP => {
            process_inner.tasks_for_each(|task| {
                let mut task_inner = task.inner_exclusive_access();
                task_inner.signal_processor.frozen = true;
            });
            process_inner.signals ^= SignalFlags::SIGSTOP;
        }
        SignalFlags::SIGCONT => {
            process_inner.tasks_for_each(|task| {
                let mut task_inner = task.inner_exclusive_access();
                task_inner.signal_processor.frozen = false;
            });
            process_inner.signals ^= SignalFlags::SIGCONT;
        }
        _ => {
            process_inner.tasks_for_each(|task| {
                let mut task_inner = task.inner_exclusive_access();
                task_inner.signal_processor.killed = true;
            });
        }
    }
}

fn call_user_signal_handler(signum: usize, signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    let handler = task_inner.signal_processor.handler_for_action(signum);
    if handler == 0 {
        return;
    }
    // handle flag
    let process = task.process.upgrade().unwrap();
    process.inner_exclusive_access().signals ^= signal;
    task_inner.signal_processor.signal_handling = Some(signum);
    // backup trapframe
    let trap_cx = task_inner.get_trap_cx();
    task_inner.signal_processor.trap_cx_backup = Some(*trap_cx);
    // modify trapframe
    trap_cx.sepc = handler;
    // put args (a0)
    trap_cx.x[10] = signum;
}

pub fn check_signals_error_of_current() -> Option<(i32, &'static str)> {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    inner.signals.check_error()
}

// pub fn remove_inactive_task(task: Arc<TaskControlBlock>) {
//     remove_task(task.clone());
//     remove_timer(&task);
// }
