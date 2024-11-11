use core::cmp::Reverse;

use alloc::{collections::binary_heap::BinaryHeap, sync::Arc};
use lazy_static::lazy_static;

use crate::sync::UPSafeCell;

use super::task::TaskControlBlock;

lazy_static! {
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

// (stride, Arc)
struct StridedArcTask(u64, Arc<TaskControlBlock>);
impl Ord for StridedArcTask {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.0.wrapping_sub(other.0) as i64).cmp(&0i64)
    }
}
impl PartialOrd for StridedArcTask {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for StridedArcTask {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}
impl Eq for StridedArcTask {}

pub struct TaskManager {
    ready_queue: BinaryHeap<Reverse<StridedArcTask>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    pub fn new() -> Self {
        Self {
            ready_queue: BinaryHeap::new(),
        }
    }

    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        let stride = task.inner_exclusive_access().stride;
        self.ready_queue.push(Reverse(StridedArcTask(stride, task)));
    }

    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue
            .pop()
            .map(|Reverse(StridedArcTask(_, task))| {
                task.inner_exclusive_access().stride_step();
                task
            })
    }
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}
