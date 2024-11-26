use alloc::{collections::vec_deque::VecDeque, sync::Arc};

use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};

use super::{Mutex, UPSafeCell};

pub struct CondVar {
    pub inner: UPSafeCell<CondVarInner>,
}

pub struct CondVarInner {
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl CondVar {
    pub fn new() -> Self {
        Self {
            inner: unsafe {
                UPSafeCell::new(CondVarInner {
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    pub fn signal(&self) {
        let mut inner = self.inner.exclusive_access();
        if let Some(task) = inner.wait_queue.pop_front() {
            wakeup_task(task);
        }
    }

    pub fn wait(&self, mutex: Arc<dyn Mutex>) {
        mutex.unlock();
        let mut inner = self.inner.exclusive_access();
        let task = current_task().unwrap();
        inner.wait_queue.push_back(task);
        drop(inner);
        block_current_and_run_next();
        mutex.lock();
    }
}
