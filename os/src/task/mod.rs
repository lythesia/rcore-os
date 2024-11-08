use alloc::vec::Vec;
use context::TaskContext;
use lazy_static::lazy_static;
use task::{TaskControlBlock, TaskStatus};

use crate::{
    loader,
    mm::{MapPermission, VPNRange, VirtPageNum},
    sbi::shutdown,
    sync::UPSafeCell,
    timer::get_time_us,
    trap::TrapContext,
};

mod context;
mod switch;
mod task;

lazy_static! {
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = loader::get_num_app();
        log::info!("init TASK_MANAGER: num_app = {}", num_app);
        let mut tasks = Vec::new();

        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(loader::get_app_data(i), i));
        }

        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    curr_task: 0,
                    stop_watch: 0,
                })
            },
        }
    };
}

pub struct TaskManager {
    num_app: usize,
    inner: UPSafeCell<TaskManagerInner>,
}

struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// id of current `Running` task
    curr_task: usize,
    /// stop watch
    stop_watch: usize,
}

impl TaskManagerInner {
    /// stop_watch <- now, return time of `last stop` until `now`
    fn refresh_stop_watch(&mut self) -> usize {
        let start = self.stop_watch;
        self.stop_watch = get_time_us();
        self.stop_watch - start
    }
}

impl TaskManager {
    fn run_first_task(&self) {
        let mut inner = self.inner.exclusive_access();

        let task0 = &mut inner.tasks[0];
        task0.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &task0.task_cx as *const TaskContext;
        // 开始记录时间, 但是第一个task之后会一直执行到suspend或exit
        inner.refresh_stop_watch();
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        unsafe {
            __switch(&mut _unused as *mut TaskContext, next_task_cx_ptr);
        }
        panic!("Unreachable in run_first_task!");
    }

    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let curr = inner.curr_task;
        // curr_task挂起, 应当停表累加kernel_time, 而__switch耗时应当不算入curr/next_task的kernel_time
        inner.tasks[curr].kernel_time += inner.refresh_stop_watch();
        // log::debug!("[kernel] Task_{} => Ready(suspended)", curr);
        inner.tasks[curr].task_status = TaskStatus::Ready;
    }

    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let curr = inner.curr_task;
        // curr_task完成, 因为task退出也是调用sys_exit, 这个syscall本身也当计入kernel_time
        inner.tasks[curr].kernel_time += inner.refresh_stop_watch();
        let user_time = inner.tasks[curr].user_time;
        let kernel_time = inner.tasks[curr].kernel_time;
        log::debug!(
            "[kernel] Task_{} => Exited | user_time = {} us, kernel_time = {} us",
            curr,
            user_time,
            kernel_time
        );
        inner.tasks[curr].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return app id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let curr = inner.curr_task;
        (curr + 1..curr + 1 + self.num_app)
            .map(|i| i % self.num_app)
            .find(|i| inner.tasks[*i].task_status == TaskStatus::Ready)
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        let next = match self.find_next_task() {
            Some(v) => v,
            _ => {
                log::debug!(
                    "[kernel] task switch time (in total): {} us",
                    get_switch_time_count()
                );
                log::info!("All applications completed!");
                shutdown(false);
            }
        };

        let mut inner = self.inner.exclusive_access();
        let curr = inner.curr_task;
        inner.tasks[next].task_status = TaskStatus::Running;
        inner.curr_task = next;
        let current_task_cx_ptr = &mut inner.tasks[curr].task_cx as *mut TaskContext;
        let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
        // 在实际切换之前我们需要手动 drop 掉我们获取到的 TaskManagerInner 的来自 UPSafeCell 的借用标记。
        // 因为一般情况下它是在函数退出之后才会被自动释放，从而 TASK_MANAGER 的 inner 字段得以回归到未被借用的状态，
        // 之后可以再借用。如果不手动 drop 的话，编译器会在 __switch 返回时，也就是当前应用被切换回来的时候才 drop，
        // 这期间我们都不能修改 TaskManagerInner ，甚至不能读（因为之前是可变借用），会导致内核 panic 报错退出。
        // 解释一下, 这里switch到别的trap控制流以后, 后者也是要访问TaskManager的, 当exclusive_acess的时候就panic:
        // borrowed already
        drop(inner);
        // log::debug!("[kernel] Task_{} => Task_{}", curr, next);
        unsafe {
            __switch(current_task_cx_ptr, next_task_cx_ptr);
        }
    }

    pub fn user_time_start(&self) {
        let mut inner = self.inner.exclusive_access();
        let curr = inner.curr_task;
        // 到user_time_start为止都是kernel_time, 故累加
        // 隐含另一个意思, 从现在开始是user_time
        inner.tasks[curr].kernel_time += inner.refresh_stop_watch();
    }

    pub fn user_time_end(&self) {
        let mut inner = self.inner.exclusive_access();
        let curr = inner.curr_task;
        // 类似上面, 到user_time_end为止都是user_time, 故累加
        // 隐含另一个意思, 从现在开始是kernel_time
        inner.tasks[curr].user_time += inner.refresh_stop_watch();
    }

    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.curr_task;
        inner.tasks[current].get_current_token()
    }

    fn get_current_trap_cx(&self) -> &mut TrapContext {
        let inner = self.inner.exclusive_access();
        let current = inner.curr_task;
        inner.tasks[current].get_trap_cx()
    }
}

pub static mut SWITCH_TIME_START: usize = 0;
pub static mut SWITCH_TIME_COUNT: usize = 0; // 记录switch的**总开销**

unsafe fn __switch(current_task_cx_ptr: *mut TaskContext, next_task_cx_ptr: *const TaskContext) {
    SWITCH_TIME_START = get_time_us();
    switch::__switch(current_task_cx_ptr, next_task_cx_ptr);
    SWITCH_TIME_COUNT += get_time_us() - SWITCH_TIME_START;
}

fn get_switch_time_count() -> usize {
    unsafe { SWITCH_TIME_COUNT }
}

pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

pub fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

pub fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

pub fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

pub fn user_time_start() {
    TASK_MANAGER.user_time_start()
}

pub fn user_time_end() {
    TASK_MANAGER.user_time_end()
}

pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

pub fn current_task_map_new_area(
    start_vpn: VirtPageNum,
    end_vpn: VirtPageNum,
    map_perm: MapPermission,
) -> isize {
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.curr_task;
    let curr_mem_set = &mut inner.tasks[current].memory_set;
    for vpn in VPNRange::new(start_vpn, end_vpn) {
        match curr_mem_set.translate(vpn) {
            Some(pte) if pte.is_valid() => return -1, // mapped already
            _ => {}
        }
    }
    curr_mem_set.insert_framed_area(start_vpn.into(), end_vpn.into(), map_perm);
    0
}

pub fn current_task_unmap_area(start_vpn: VirtPageNum, end_vpn: VirtPageNum) -> isize {
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.curr_task;
    let curr_mem_set = &mut inner.tasks[current].memory_set;
    for vpn in VPNRange::new(start_vpn, end_vpn) {
        match curr_mem_set.translate(vpn) {
            Some(pte) if !pte.is_valid() => return -1, // not valid
            Some(_) => curr_mem_set.page_table_mut().unmap(vpn),
            _ => return -1, // no entry
        }
    }
    0
}
