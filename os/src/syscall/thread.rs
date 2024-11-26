use alloc::sync::Arc;

use crate::{
    mm,
    task::{self, add_task, TaskControlBlock},
    trap::{trap_handler, TrapContext},
};

// create_thread(void *func_ptr, void *arg);
pub fn sys_thread_create(entry: usize, arg: usize) -> isize {
    let task = task::current_task().unwrap();
    let process = task.process.upgrade().unwrap();
    // create new TCB
    let new_task = Arc::new(TaskControlBlock::new(
        process.clone(),
        task.inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .ustack_base,
        true,
    ));
    // add new thread to scheduler
    add_task(new_task.clone());

    let new_task_inner = new_task.inner_exclusive_access();
    let new_task_res = new_task_inner.res.as_ref().unwrap();
    let new_task_id = new_task_res.tid;

    // add new thread to process
    let mut process_inner = process.inner_exclusive_access();
    let tasks = &mut process_inner.tasks;
    while tasks.len() < new_task_id + 1 {
        tasks.push(None);
    }
    tasks[new_task_id] = Some(new_task.clone()); // when it's A recycled id, then old TaskUserRes is finally dropped

    let new_task_trap_cx = new_task_inner.get_trap_cx();
    *new_task_trap_cx = TrapContext::app_init_context(
        entry,
        new_task_res.ustack_top(),
        mm::kernel_token(),
        new_task.kstack.get_top(),
        trap_handler as usize,
    );
    // set a0 for new thread
    (*new_task_trap_cx).x[10] = arg;
    new_task_id as isize
}

pub fn sys_gettid() -> isize {
    task::current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid as isize
}

pub fn sys_waittid(tid: usize) -> i32 {
    let task = task::current_task().unwrap();
    let process = task.process.upgrade().unwrap();
    let task_inner = task.inner_exclusive_access();
    let mut process_inner = process.inner_exclusive_access();
    // a thread cannot wait for itself
    if task_inner.res.as_ref().unwrap().tid == tid {
        return -1;
    }
    // sometimes thread run-and-exit(sys_exit) too fast, TaskUserRes got dropped as well as tid
    // so new created thread reuses the tid
    // say we want to create th-1,2,3, after 1,2 created, before 3 create, 1,2 fin and sys_exit
    // so th-3 reuses (maybe) 2; then waittid(1) -> ok with exit1, waittid(2)(we want 2 but actually 3) we got exit3 here,
    // wattid(2), we want 3 but exit3 already returned and dealloc, so this waittid(2) got None -> -1
    // conclusion: we should not recycle tid too early, or until wait success or process exit
    let exit_code = match process_inner.tasks.get(tid) {
        Some(Some(t)) => t.inner_exclusive_access().exit_code.clone(),
        _ => return -1, // tid not exist
    };
    if let Some(exit_code) = exit_code {
        // dealloc exited thread
        process_inner.tasks[tid] = None;
        process_inner.dealloc_tid(tid);
        exit_code
    } else {
        -2
    }
}
