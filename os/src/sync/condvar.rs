use alloc::{collections::vec_deque::VecDeque, sync::Arc};

use crate::task::{
    block_current_and_run_next, block_current_task, current_task, wakeup_task, TaskContext,
    TaskControlBlock,
};

use super::{Mutex, UPIntrFreeCell};

pub struct Condvar {
    pub inner: UPIntrFreeCell<CondVarInner>,
}

pub struct CondVarInner {
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Condvar {
    pub fn new() -> Self {
        Self {
            inner: unsafe {
                UPIntrFreeCell::new(CondVarInner {
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

    pub fn wait_no_sched(&self) -> *mut TaskContext {
        self.inner.exclusive_session(|inner| {
            inner.wait_queue.push_back(current_task().unwrap());
        });
        block_current_task()
    }
}
