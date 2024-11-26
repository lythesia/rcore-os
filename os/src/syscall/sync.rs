use alloc::sync::Arc;

use crate::{
    sync::{CondVar, Mutex, MutexBlocking, MutexSpin, Semaphore},
    task, timer,
};

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = timer::get_time_ms() + ms;
    let task = task::current_task().unwrap();
    timer::add_timer(expire_ms, task);
    task::block_current_and_run_next();
    0
}

pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = task::current_process();
    let mutex: Option<Arc<dyn Mutex>> = if blocking {
        Some(Arc::new(MutexBlocking::new()))
    } else {
        Some(Arc::new(MutexSpin::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(idx) = process_inner.mutex_list.iter().position(|v| v.is_none()) {
        process_inner.mutex_list[idx] = mutex;
        idx as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() as isize - 1
    }
}

pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let process = task::current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = match process_inner.mutex_list.get(mutex_id) {
        Some(Some(v)) => v.clone(),
        _ => return -1, // mutex not exist
    };
    drop(process_inner);
    drop(process);
    mutex.lock();
    0
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = task::current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = match process_inner.mutex_list.get(mutex_id) {
        Some(Some(v)) => v.clone(),
        _ => return -1, // mutex not exist
    };
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = task::current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Some(Arc::new(Semaphore::new(res_count)));
    if let Some(idx) = process_inner
        .semaphore_list
        .iter()
        .position(|v| v.is_none())
    {
        process_inner.semaphore_list[idx] = sem;
        idx as isize
    } else {
        process_inner.semaphore_list.push(sem);
        process_inner.semaphore_list.len() as isize - 1
    }
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = task::current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = match process_inner.semaphore_list.get(sem_id) {
        Some(Some(v)) => v.clone(),
        _ => return -1, // sem not exist
    };
    drop(process_inner);
    drop(process);
    sem.up();
    0
}

pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let process = task::current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = match process_inner.semaphore_list.get(sem_id) {
        Some(Some(v)) => v.clone(),
        _ => return -1, // sem not exist
    };
    drop(process_inner);
    drop(process);
    sem.down();
    0
}

pub fn sys_condvar_create() -> isize {
    let process = task::current_process();
    let mut process_inner = process.inner_exclusive_access();
    let cv = Some(Arc::new(CondVar::new()));
    if let Some(idx) = process_inner.condvar_list.iter().position(|v| v.is_none()) {
        process_inner.condvar_list[idx] = cv;
        idx as isize
    } else {
        process_inner.condvar_list.push(cv);
        process_inner.condvar_list.len() as isize - 1
    }
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = task::current_process();
    let process_inner = process.inner_exclusive_access();
    let cv = match process_inner.condvar_list.get(condvar_id) {
        Some(Some(v)) => v.clone(),
        _ => return -1, // cv not exist
    };
    drop(process_inner);
    drop(process);
    cv.signal();
    0
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let process = task::current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = match process_inner.mutex_list.get(mutex_id) {
        Some(Some(v)) => v.clone(),
        _ => return -1, // mutex not exist
    };
    let cv = match process_inner.condvar_list.get(condvar_id) {
        Some(Some(v)) => v.clone(),
        _ => return -1, // cv not exist
    };
    drop(process_inner);
    drop(process);
    cv.wait(mutex);
    0
}
