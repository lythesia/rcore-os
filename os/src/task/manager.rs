use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    sync::Arc,
};
use lazy_static::lazy_static;

use crate::sync::UPSafeCell;

use super::task::TaskControlBlock;

lazy_static! {
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
    pub static ref PID2TCB: UPSafeCell<BTreeMap<usize, Arc<TaskControlBlock>>> =
        unsafe { UPSafeCell::new(BTreeMap::new()) };
}

pub struct TaskManager {
    /// 使用Arc而非TCB本身, 因为经常需要被放入/取出, 拷贝开销较大, 故使用Arc指针; 其次Arc共享更方便
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
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    PID2TCB
        .exclusive_access()
        .insert(task.getpid(), task.clone());
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}

pub fn pid2task(pid: usize) -> Option<Arc<TaskControlBlock>> {
    let map = PID2TCB.exclusive_access();
    map.get(&pid).map(Arc::clone)
}

pub fn remove_from_pid2task(pid: usize) {
    if PID2TCB.exclusive_access().remove(&pid).is_none() {
        panic!("cannot find pid {} in pid2task!", pid);
    }
}
