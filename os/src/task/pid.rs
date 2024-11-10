use alloc::vec::Vec;
use lazy_static::lazy_static;

use crate::{
    config::{KERNEL_STACK_SIZE, PAGE_SIZE, TRAMPOLINE},
    mm::{MapPermission, VirtAddr, KERNEL_SPACE},
    sync::UPSafeCell,
};

lazy_static! {
    static ref PID_ALLOCATOR: UPSafeCell<PidAllocator> =
        unsafe { UPSafeCell::new(PidAllocator::new()) };
}

pub struct PidHandle(pub usize);
impl From<usize> for PidHandle {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
impl Drop for PidHandle {
    fn drop(&mut self) {
        PID_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

struct PidAllocator {
    current: usize,
    recycled: Vec<usize>,
}
impl PidAllocator {
    pub fn new() -> Self {
        Self {
            current: 0,
            recycled: Vec::new(),
        }
    }

    pub fn alloc(&mut self) -> PidHandle {
        match self.recycled.pop() {
            Some(v) => v.into(),
            _ => {
                let v = self.current;
                self.current += 1;
                v.into()
            }
        }
    }

    pub fn dealloc(&mut self, pid: usize) {
        assert!(pid < self.current);
        assert!(
            self.recycled.iter().find(|&v| v == &pid).is_none(),
            "pid {} has been deallocated!",
            pid
        );
        self.recycled.push(pid);
    }
}

/// allocate pid
pub fn pid_alloc() -> PidHandle {
    PID_ALLOCATOR.exclusive_access().alloc()
}

pub struct KernelStack {
    pid: usize,
}

/// Return (bottom, top) of a kernel stack in kernel space.
pub fn kernel_stack_position(app_id: usize) -> (usize, usize) {
    // 每个app的kernel stack大小固定, 且从高到低依次排列
    // 每个app间使用一个guard page来分隔, 即这里为什么要+ PAGE_SIZE
    let top = TRAMPOLINE - app_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

impl KernelStack {
    pub fn new(pid_handle: &PidHandle) -> Self {
        let pid = pid_handle.0;
        let (kstack_bottom, kstack_top) = kernel_stack_position(pid);
        KERNEL_SPACE.exclusive_access().insert_framed_area(
            kstack_bottom.into(),
            kstack_top.into(),
            MapPermission::R | MapPermission::W,
        );
        KernelStack { pid }
    }

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
        let (_, kstack_top) = kernel_stack_position(self.pid);
        kstack_top
    }
}

impl Drop for KernelStack {
    fn drop(&mut self) {
        let (kstack_bottom, _) = kernel_stack_position(self.pid);
        let kstack_bottom_va: VirtAddr = kstack_bottom.into();
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kstack_bottom_va.into());
    }
}
