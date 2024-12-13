use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    sync::Arc,
};
use lazy_static::lazy_static;

use crate::sync::UPIntrFreeCell;

use super::{
    process::ProcessControlBlock,
    task::{TaskControlBlock, TaskStatus},
};

lazy_static! {
    pub static ref TASK_MANAGER: UPIntrFreeCell<TaskManager> =
        unsafe { UPIntrFreeCell::new(TaskManager::new()) };
    pub static ref PID2PCB: UPIntrFreeCell<BTreeMap<usize, Arc<ProcessControlBlock>>> =
        unsafe { UPIntrFreeCell::new(BTreeMap::new()) };
}

pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }

    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }

    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue.pop_front()
    }

    // pub fn remove(&mut self, task: Arc<TaskControlBlock>) {
    //     if let Some((id, _)) = self
    //         .ready_queue
    //         .iter()
    //         .enumerate()
    //         .find(|(_, t)| Arc::as_ptr(t) == Arc::as_ptr(&task))
    //     {
    //         self.ready_queue.remove(id);
    //     }
    // }
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}

pub fn wakeup_task(task: Arc<TaskControlBlock>) {
    let mut inner = task.inner_exclusive_access();
    inner.task_status = TaskStatus::Ready;
    drop(inner);
    add_task(task);
}

// pub fn remove_task(task: Arc<TaskControlBlock>) {
//     TASK_MANAGER.exclusive_access().remove(task);
// }

pub fn insert_into_pid2process(pid: usize, process: Arc<ProcessControlBlock>) {
    PID2PCB.exclusive_access().insert(pid, process);
}

pub fn pid2process(pid: usize) -> Option<Arc<ProcessControlBlock>> {
    PID2PCB.exclusive_access().get(&pid).map(Arc::clone)
}

pub fn remove_from_pid2process(pid: usize) {
    if PID2PCB.exclusive_access().remove(&pid).is_none() {
        panic!("cannot find pid {} in pid2task!", pid);
    }
}
