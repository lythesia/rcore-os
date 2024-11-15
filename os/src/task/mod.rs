use alloc::sync::Arc;
use context::TaskContext;
use lazy_static::lazy_static;
use task::{TaskControlBlock, TaskStatus};

use crate::fs;

mod context;
mod manager;
mod mem;
mod pid;
mod processor;
mod switch;
mod task;

pub use manager::add_task;
pub use mem::*;
pub use processor::{
    current_task, current_trap_cx, current_user_token, run_tasks, user_time_end, user_time_start,
};

lazy_static! {
    pub static ref INITPROC: Arc<TaskControlBlock> = Arc::new({
        let inode = fs::open_file("initproc", fs::OpenFlags::RDONLY).unwrap();
        let elf = inode.read_all();
        TaskControlBlock::new(&elf)
    });
}

pub fn suspend_current_and_run_next() {
    // There must be an application running.
    let task = processor::take_current_task().unwrap();

    // 当仅有一个任务的时候, suspend_current_and_run_next 的效果是会继续执行这个任务
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // curr_task挂起, 应当停表累加kernel_time, 而__switch耗时应当不算入curr/next_task的kernel_time
    task_inner.kernel_time += processor::refresh_stop_watch();
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);

    add_task(task); // enqueue to manager
    processor::schedule(task_cx_ptr);
}

const IDLE_PID: usize = 0;
pub fn exit_current_and_run_next(exit_code: i32) {
    // There must be an application running.
    let task = processor::take_current_task().unwrap();

    if task.getpid() == IDLE_PID {
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

    let mut task_inner = task.inner_exclusive_access();
    // curr_task完成
    task_inner.kernel_time += processor::refresh_stop_watch();
    task_inner.task_status = TaskStatus::Zombie;
    task_inner.exit_code = exit_code;

    // access initproc TCB exclusively
    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in task_inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }

    task_inner.children.clear();
    // deallocate user space
    task_inner.memory_set.recycle_data_pages();
    drop(task_inner);

    // drop task manually to maintain rc correctly
    // drop以后task的Arc不再存在于TaskManager nor Processor, 只可能在其parent的children vec中, 如此waitpid调用才能
    // 从自己的children中找到zombie的task, 获取其最后有用的信息, 比如exit_code, kernel/user_time等
    drop(task);
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
    add_task(INITPROC.clone());
}
