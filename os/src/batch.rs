use core::arch::asm;

use lazy_static::lazy_static;

use crate::{println, sync::UPSafeCell, trap::TrapContext};

const MAX_APP_NUM: usize = 16;
const APP_BASE_ADDRESS: usize = 0x80400000;
const APP_SIZE_LIMIT: usize = 0x20000;
// both user & kernel stack 8KB
const USER_STACK_SIZE: usize = 4096 * 2;
const KERNEL_STACK_SIZE: usize = 4096 * 2;

#[repr(align(4096))]
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
struct UserStack {
    data: [u8; USER_STACK_SIZE],
}

impl UserStack {
    fn get_sp(&self) -> usize {
        // stack grows downwards, so stack top ==  base + size
        self.data.as_ptr() as usize + USER_STACK_SIZE
    }
}

static KERNEL_STACK: KernelStack = KernelStack {
    data: [0; KERNEL_STACK_SIZE],
};
static USER_STACK: UserStack = UserStack {
    data: [0; USER_STACK_SIZE],
};

lazy_static! {
    static ref APP_MANAGER: UPSafeCell<AppManager> = unsafe {
        UPSafeCell::new({
            extern "C" {
                fn _num_app();
            }

            let ptr = _num_app as usize as *const usize;
            let num_app = ptr.read_volatile();

            // why read 1 more?
            let mut app_start = [0usize; MAX_APP_NUM + 1];
            let app_start_raw = core::slice::from_raw_parts(ptr.add(1), num_app + 1);
            // so last is `app_x_end`?
            // because need to compute area of app:
            // [app_src, app_dst] = [app_{i}_start, app_{i+1}_start]
            // = [app_{i}_start, app_{i}_end] for last one
            app_start[..=num_app].copy_from_slice(app_start_raw);
            AppManager {
                num_app,
                curr_app: 0,
                app_start,
            }
        })
    };
}

struct AppManager {
    num_app: usize,
    curr_app: usize,
    app_start: [usize; MAX_APP_NUM + 1],
}

impl AppManager {
    // 在init时被调用, 完成AppManager初始化
    pub fn print_app_info(&self) {
        println!("[kernel] num_app = {}", self.num_app);
        for (i, [app_s, app_e]) in self
            .app_start
            .array_windows::<2>()
            .take(self.num_app)
            .enumerate()
        {
            println!("[kernel] app_{}: [{:#x}, {:#x}]", i, app_s, app_e);
        }
    }

    unsafe fn load_app(&self, app_id: usize) {
        let [src, dst] = self
            .app_start
            .array_windows::<2>()
            .take(self.num_app)
            .nth(app_id)
            .expect("All application completed!");

        println!("[kernel] Loading app_{}", app_id);
        // clear app area
        core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8, APP_SIZE_LIMIT).fill(0);
        let app_src = core::slice::from_raw_parts(*src as *const u8, dst - src);
        let app_dst = core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8, app_src.len());
        app_dst.copy_from_slice(app_src);
        // memory fence about fetching the instruction memory
        asm!("fence.i");
    }

    fn get_current_app(&self) -> usize {
        self.curr_app
    }

    fn move_to_next_app(&mut self) {
        self.curr_app += 1;
    }
}

pub fn init() {
    print_app_info();
}

pub fn print_app_info() {
    APP_MANAGER.exclusive_access().print_app_info();
}

pub fn run_next_app() -> ! {
    let mut mgr = APP_MANAGER.exclusive_access();
    let curr_app = mgr.get_current_app();
    unsafe {
        mgr.load_app(curr_app);
    }
    mgr.move_to_next_app();
    drop(mgr);

    // before this we have to drop local variables related to resources manually
    // and release the resources
    extern "C" {
        fn __restore(cx_addr: usize);
    }
    unsafe {
        __restore(KERNEL_STACK.push_context(TrapContext::app_init_context(
            APP_BASE_ADDRESS,
            USER_STACK.get_sp(),
        )) as *const _ as usize);
    }
    panic!("Unreachable in batch::run_current_app!");
}
