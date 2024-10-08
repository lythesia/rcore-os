use core::arch::asm;

use crate::{config::*, trap::TrapContext};

#[repr(align(4096))]
#[derive(Clone, Copy)]
struct KernelStack {
    data: [u8; KERNEL_STACK_SIZE],
}

impl KernelStack {
    fn get_sp(&self) -> usize {
        // stack grows downwards, so stack top ==  base + size
        self.data.as_ptr() as usize + KERNEL_STACK_SIZE
    }

    fn push_context(&self, cx: TrapContext) -> &'static mut TrapContext {
        // Hi | sp
        //    | cx-pushed
        //    | ..
        // Lo | base
        let cx_ptr = (self.get_sp() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe {
            *cx_ptr = cx;
        }
        // 返回push的cx的地址, 也即内核栈的栈顶, 作为参数(放在a0)传给__restore
        // 这就解释了__restore开头: mv sp, a0
        // 令sp指向内核栈顶
        unsafe { cx_ptr.as_mut().unwrap() }
    }
}

#[repr(align(4096))]
#[derive(Clone, Copy)]
struct UserStack {
    data: [u8; USER_STACK_SIZE],
}

impl UserStack {
    fn get_sp(&self) -> usize {
        // stack grows downwards, so stack top ==  base + size
        self.data.as_ptr() as usize + USER_STACK_SIZE
    }
}

// !!每个App拥有各自的kernel和user stack
static KERNEL_STACK: [KernelStack; MAX_APP_NUM] = [KernelStack {
    data: [0; KERNEL_STACK_SIZE],
}; MAX_APP_NUM];
static USER_STACK: [UserStack; MAX_APP_NUM] = [UserStack {
    data: [0; USER_STACK_SIZE],
}; MAX_APP_NUM];

extern "C" {
    fn _num_app();
}

/// Load nth user app at
/// [APP_BASE_ADDRESS + n * APP_SIZE_LIMIT, APP_BASE_ADDRESS + (n+1) * APP_SIZE_LIMIT).
pub fn load_apps() {
    let ptr = _num_app as usize as *const usize;
    let num_app = get_num_app();

    let app_start = unsafe { core::slice::from_raw_parts(ptr.add(1), num_app + 1) };
    for i in 0..num_app {
        let base = get_base_i(i);
        // clear memory area for app
        unsafe {
            core::slice::from_raw_parts_mut(base as *mut u8, APP_SIZE_LIMIT).fill(0);
        }
        // load app from data section to memory
        let src = unsafe {
            core::slice::from_raw_parts(app_start[i] as *const u8, app_start[i + 1] - app_start[i])
        };
        let dst = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, src.len()) };
        dst.copy_from_slice(src);
        log::debug!(
            "[kernel] Loaded app_{}: [{:#x}, {:#x}] to {:#x}",
            i,
            app_start[i],
            app_start[i + 1],
            base
        );
    }
    unsafe {
        asm!("fence.i");
    }
}

/// Get the total number of applications.
pub fn get_num_app() -> usize {
    unsafe { (_num_app as usize as *const usize).read_volatile() }
}

/// Get base address of app i.
pub fn get_base_i(i: usize) -> usize {
    APP_BASE_ADDRESS + i * APP_SIZE_LIMIT
}

/// get app info with entry and sp and save `TrapContext` in kernel stack
pub fn init_app_cx(i: usize) -> usize {
    KERNEL_STACK[i].push_context(TrapContext::app_init_context(
        get_base_i(i),
        USER_STACK[i].get_sp(),
    )) as *const _ as usize
}
