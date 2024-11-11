use core::cell::RefMut;

use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};

use crate::{
    config::{self, TRAP_CONTEXT},
    mm::{MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE},
    sync::UPSafeCell,
    trap::{trap_handler, TrapContext},
};

use super::{
    context::TaskContext,
    pid::{pid_alloc, KernelStack, PidHandle},
};

#[derive(Clone, Copy, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Zombie,
}

/// TCB
pub struct TaskControlBlock {
    // immutable
    pub pid: PidHandle,
    pub kstack: KernelStack,
    // mutable
    inner: UPSafeCell<TaskControlBlockInner>, // use `UPSafeCell` to provide `&self` only to external
}

pub struct TaskControlBlockInner {
    pub task_status: TaskStatus,
    pub task_cx: TaskContext,
    pub memory_set: MemorySet,
    pub trap_cx_ppn: PhysPageNum,
    #[allow(unused)]
    pub base_size: usize,
    pub parent: Option<Weak<TaskControlBlock>>,
    pub children: Vec<Arc<TaskControlBlock>>,
    pub exit_code: i32,

    // time stats
    pub user_time: usize,
    pub kernel_time: usize,

    // stride
    // https://nankai.gitbook.io/ucore-os-on-risc-v64/lab6/tiao-du-suan-fa-kuang-jia#stride-suan-fa
    pub stride: u64,
    pub prio: u64,
}

impl TaskControlBlockInner {
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }

    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }

    fn get_status(&self) -> TaskStatus {
        self.task_status
    }

    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }

    pub fn stride_step(&mut self) {
        self.stride = self.stride.wrapping_add(config::STRIDE_MAX / self.prio);
    }
}

impl TaskControlBlock {
    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        // alloc pid & kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kstack = KernelStack::new(&pid_handle);
        let kstack_top = kstack.get_top();

        let task_control_block = Self {
            pid: pid_handle,
            kstack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    task_status: TaskStatus::Ready,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    memory_set,
                    trap_cx_ppn,
                    base_size: user_sp,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    user_time: 0,
                    kernel_time: 0,
                    stride: 0,
                    prio: 16,
                })
            },
        };
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        // println!("task {} borrowed", self.pid.0);
        self.inner.exclusive_access()
    }

    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    pub fn fork(self: &Arc<TaskControlBlock>) -> Arc<TaskControlBlock> {
        let mut parent_inner = self.inner_exclusive_access();
        // copy user space(include trap context)
        let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        // alloc pid & kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kstack = KernelStack::new(&pid_handle);
        let kstack_top = kstack.get_top();
        // construct TCB
        let task_control_block = Arc::new(Self {
            pid: pid_handle,
            kstack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    task_status: TaskStatus::Ready,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    memory_set,
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    user_time: 0,
                    kernel_time: 0,
                    stride: 0,
                    prio: 16,
                })
            },
        });
        // add to parent
        parent_inner.children.push(task_control_block.clone());

        // modify kernel_sp in trap_cx
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kstack_top;

        task_control_block
    }

    pub fn exec(&self, elf_data: &[u8]) {
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();

        // access inner exclusively
        let mut inner = self.inner_exclusive_access();
        // substitutes
        inner.memory_set = memory_set; // 原有的地址空间会被回收(包括物理frame)
        inner.trap_cx_ppn = trap_cx_ppn;
        // init trap_cx
        let trap_cx = inner.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kstack.get_top(),
            trap_handler as usize,
        );
    }

    pub fn spawn(self: &Arc<TaskControlBlock>, elf_data: &[u8]) -> Arc<TaskControlBlock> {
        // load elf and do mappings
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();

        // alloc pid & kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kstack = KernelStack::new(&pid_handle);
        let kstack_top = kstack.get_top();

        // access inner exclusively
        let mut parent_inner = self.inner_exclusive_access();

        // construct TCB
        let task_control_block = Arc::new(Self {
            pid: pid_handle,
            kstack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    task_status: TaskStatus::Ready,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    memory_set,
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    user_time: 0,
                    kernel_time: 0,
                    stride: 0,
                    prio: 16,
                })
            },
        });
        // add to parent
        parent_inner.children.push(task_control_block.clone());

        // init trap_cx
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );

        task_control_block
    }
}
