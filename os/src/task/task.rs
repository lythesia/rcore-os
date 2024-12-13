use alloc::sync::{Arc, Weak};

use crate::{
    mm::PhysPageNum,
    sync::{UPIntrFreeCell, UPIntrRefMut},
    trap::TrapContext,
};

use super::{
    context::TaskContext,
    id::{kstack_alloc, KernelStack, TaskUserRes},
    process::ProcessControlBlock,
    SignalActions, SignalFlags,
};

pub struct TaskControlBlock {
    // immutable
    pub process: Weak<ProcessControlBlock>,
    pub kstack: KernelStack,
    // mutable
    pub inner: UPIntrFreeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        allow_user_res: bool,
    ) -> Self {
        let process_weak = Arc::downgrade(&process);
        let res = TaskUserRes::new(process, ustack_base, allow_user_res);
        let trap_cx_ppn = res.trap_cx_ppn();
        let kstack = kstack_alloc();
        let kstack_stop = kstack.get_top();
        Self {
            process: process_weak,
            kstack,
            inner: unsafe {
                UPIntrFreeCell::new(TaskControlBlockInner {
                    res: Some(res),
                    trap_cx_ppn,
                    task_cx: TaskContext::goto_trap_return(kstack_stop),
                    task_status: TaskStatus::Ready,
                    exit_code: None,
                    signal_processor: SignalProcessor::new(),
                })
            },
        }
    }

    pub fn inner_exclusive_access(&self) -> UPIntrRefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }

    pub fn get_user_token(&self) -> usize {
        let process = self.process.upgrade().unwrap();
        let inner = process.inner_exclusive_access();
        inner.memory_set.token()
    }
}

pub struct TaskControlBlockInner {
    pub res: Option<TaskUserRes>,
    pub trap_cx_ppn: PhysPageNum,
    pub task_cx: TaskContext,
    pub task_status: TaskStatus,
    pub exit_code: Option<i32>,
    pub signal_processor: SignalProcessor,
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }

    // fn get_status(&self) -> TaskStatus {
    //     self.task_status
    // }
}

#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Blocked,
}

pub struct SignalProcessor {
    // current handling signal(signum)
    pub signal_handling: Option<usize>,
    // global mask
    pub signal_mask: SignalFlags,
    // (re)action of signals
    pub signal_actions: SignalActions,
    // status killed
    pub killed: bool,
    // status frozen
    pub frozen: bool,
    // backup trap_cx (when handling signal)
    pub trap_cx_backup: Option<TrapContext>,
}

impl SignalProcessor {
    pub fn new() -> Self {
        Self {
            signal_handling: None,
            signal_mask: SignalFlags::empty(),
            signal_actions: SignalActions::default(),
            killed: false,
            frozen: false,
            trap_cx_backup: None,
        }
    }

    pub fn is_global_masked(&self, signal: SignalFlags) -> bool {
        self.signal_mask.contains(signal)
    }

    pub fn is_handling_masked(&self, signal: SignalFlags) -> bool {
        match &self.signal_handling {
            Some(signum) => self.signal_actions.is_masked(*signum, signal),
            _ => false,
        }
    }

    pub fn handler_for_action(&self, signum: usize) -> usize {
        self.signal_actions.get_handler(signum)
    }
}
