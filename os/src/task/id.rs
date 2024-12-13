use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use lazy_static::lazy_static;

use crate::{
    config::{KERNEL_STACK_SIZE, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT, USER_STACK_SIZE},
    mm::{MapPermission, PhysPageNum, VirtAddr, KERNEL_SPACE},
    sync::UPIntrFreeCell,
};

use super::ProcessControlBlock;

lazy_static! {
    static ref PID_ALLOCATOR: UPIntrFreeCell<RecycleAllocator> =
        unsafe { UPIntrFreeCell::new(RecycleAllocator::new()) };
    static ref KSTACK_ALLOCATOR: UPIntrFreeCell<RecycleAllocator> =
        unsafe { UPIntrFreeCell::new(RecycleAllocator::new()) };
}
pub struct RecycleAllocator {
    current: usize,
    recycled: Vec<usize>,
}
impl RecycleAllocator {
    pub fn new() -> Self {
        Self {
            current: 0,
            recycled: Vec::new(),
        }
    }

    pub fn alloc(&mut self) -> usize {
        match self.recycled.pop() {
            Some(v) => v.into(),
            _ => {
                let v = self.current;
                self.current += 1;
                v
            }
        }
    }

    pub fn dealloc(&mut self, id: usize) {
        assert!(id < self.current);
        assert!(
            self.recycled.iter().find(|&v| v == &id).is_none(),
            "id {} has been deallocated!",
            id
        );
        self.recycled.push(id);
    }
}

pub const IDLE_PID: usize = 0;

pub struct PidHandle(pub usize);

/// allocate pid
pub fn pid_alloc() -> PidHandle {
    PidHandle(PID_ALLOCATOR.exclusive_access().alloc())
}

impl Drop for PidHandle {
    fn drop(&mut self) {
        PID_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

pub struct KernelStack(pub usize);

/// Return (bottom, top) of a kernel stack in kernel space.
pub fn kernel_stack_position(kstack_id: usize) -> (usize, usize) {
    // 每个app的kernel stack大小固定, 且从高到低依次排列
    // 每个app间使用一个guard page来分隔, 即这里为什么要+ PAGE_SIZE
    let top = TRAMPOLINE - kstack_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

pub fn kstack_alloc() -> KernelStack {
    let kstack_id = KSTACK_ALLOCATOR.exclusive_access().alloc();
    let (kstack_bottom, kstack_top) = kernel_stack_position(kstack_id);
    KERNEL_SPACE.exclusive_access().insert_framed_area(
        kstack_bottom.into(),
        kstack_top.into(),
        MapPermission::R | MapPermission::W,
    );
    KernelStack(kstack_id)
}

impl Drop for KernelStack {
    fn drop(&mut self) {
        let (kstack_bottom, _) = kernel_stack_position(self.0);
        let kstack_bottom_va: VirtAddr = kstack_bottom.into();
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kstack_bottom_va.into());
    }
}

impl KernelStack {
    #[allow(unused)]
    pub fn push_on_top<T>(&self, value: T) -> *mut T
    where
        T: Sized,
    {
        let kstack_top = self.get_top();
        let ptr_mut = (kstack_top - core::mem::size_of::<T>()) as *mut T;
        unsafe {
            *ptr_mut = value;
        }
        ptr_mut
    }

    pub fn get_top(&self) -> usize {
        let (_, kstack_top) = kernel_stack_position(self.0);
        kstack_top
    }
}

pub struct TaskUserRes {
    pub tid: usize,
    pub ustack_base: usize,
    pub process: Weak<ProcessControlBlock>,
}

fn trap_cx_bottom_from_tid(tid: usize) -> usize {
    TRAP_CONTEXT - tid * PAGE_SIZE
}

fn ustack_bottom_from_tid(ustack_base: usize, tid: usize) -> usize {
    ustack_base + tid * (PAGE_SIZE + USER_STACK_SIZE)
}

impl TaskUserRes {
    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        allow_user_res: bool,
    ) -> Self {
        let tid = process.inner_exclusive_access().alloc_tid();
        let task_user_res = Self {
            tid,
            ustack_base,
            process: Arc::downgrade(&process),
        };
        if allow_user_res {
            task_user_res.alloc_user_res();
        }
        task_user_res
    }

    pub fn alloc_user_res(&self) {
        let process = self.process.upgrade().unwrap();
        let mut inner = process.inner_exclusive_access();
        // alloc ustack
        let ustack_bottom = ustack_bottom_from_tid(self.ustack_base, self.tid);
        let ustack_top = ustack_bottom + USER_STACK_SIZE;
        inner.memory_set.insert_framed_area(
            ustack_bottom.into(),
            ustack_top.into(),
            MapPermission::R | MapPermission::W | MapPermission::U,
        );
        // alloc trap_cx
        let trap_cx_bottom = trap_cx_bottom_from_tid(self.tid);
        let trap_cx_top = trap_cx_bottom + PAGE_SIZE;
        inner.memory_set.insert_framed_area(
            trap_cx_bottom.into(),
            trap_cx_top.into(),
            MapPermission::R | MapPermission::W,
        );
    }

    fn dealloc_user_res(&self) {
        let process = self.process.upgrade().unwrap();
        let mut inner = process.inner_exclusive_access();
        // dealloc ustack
        let ustack_bottom_va: VirtAddr = ustack_bottom_from_tid(self.ustack_base, self.tid).into();
        inner
            .memory_set
            .remove_area_with_start_vpn(ustack_bottom_va.into());
        // dealloc trap_cx
        let trap_cx_bottom_va: VirtAddr = trap_cx_bottom_from_tid(self.tid).into();
        inner
            .memory_set
            .remove_area_with_start_vpn(trap_cx_bottom_va.into());
    }

    pub fn trap_cx_user_va(&self) -> usize {
        trap_cx_bottom_from_tid(self.tid)
    }

    pub fn trap_cx_ppn(&self) -> PhysPageNum {
        let process = self.process.upgrade().unwrap();
        let inner = process.inner_exclusive_access();
        let trap_cx_bottom_va: VirtAddr = trap_cx_bottom_from_tid(self.tid).into();
        inner
            .memory_set
            .translate(trap_cx_bottom_va.into())
            .unwrap()
            .ppn()
    }

    pub fn ustack_base(&self) -> usize {
        self.ustack_base
    }

    pub fn ustack_top(&self) -> usize {
        ustack_bottom_from_tid(self.ustack_base, self.tid) + USER_STACK_SIZE
    }
}

impl Drop for TaskUserRes {
    fn drop(&mut self) {
        // we should not recycle tid too earlyeven thread exited, see sys_waittid comments
        // self.dealloc_tid();
        self.dealloc_user_res();
    }
}
