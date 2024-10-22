use context::TaskContext;
use lazy_static::lazy_static;
use task::{TaskControlBlock, TaskStatus};

use crate::{
    config::{MAX_APP_NUM, MAX_SYSCALL_NUM},
    loader,
    sbi::shutdown,
    sync::UPSafeCell,
    timer::get_time_us,
};

mod context;
mod switch;
mod task;

lazy_static! {
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = loader::get_num_app();
        let mut tasks = [TaskControlBlock {
            task_status: task::TaskStatus::UnInit,
            task_cx: TaskContext::zero_init(),
            user_time: 0,
            kernel_time: 0,
            syscall_times: [0; MAX_SYSCALL_NUM],
        }; MAX_APP_NUM];

        for (i, task) in tasks.iter_mut().enumerate() {
            task.task_cx = TaskContext::goto_restore(loader::init_app_cx(i));
            task.task_status = TaskStatus::Ready;
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
    tasks: [TaskControlBlock; MAX_APP_NUM],
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

    // fn current_task(&self) -> usize {
    //     let inner = self.inner.exclusive_access();
    //     inner.curr_task
    // }

    fn current_task_status(&self) -> TaskStatus {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.curr_task].task_status
    }

    fn current_task_run_time(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let curr = &inner.tasks[inner.curr_task];
        curr.kernel_time + curr.user_time
    }

    fn record_syscall(&self, id: usize) {
        let mut inner = self.inner.exclusive_access();
        let curr = inner.curr_task;
        inner.tasks[curr].syscall_times[id] += 1;
    }

    // todo: opt copy?
    fn current_task_syscall_times(&self) -> [usize; MAX_SYSCALL_NUM] {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.curr_task].syscall_times.clone()
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

#[repr(C)]
pub struct TaskInfo {
    pub status: TaskStatus,
    pub call: [usize; MAX_SYSCALL_NUM],
    pub time: usize,
}

// pub fn get_current_task_id() -> usize {
//     TASK_MANAGER.current_task()
// }

pub fn get_current_task_status() -> TaskStatus {
    TASK_MANAGER.current_task_status()
}

pub fn get_current_task_run_time() -> usize {
    TASK_MANAGER.current_task_run_time()
}

pub fn current_task_record_syscall(id: usize) {
    TASK_MANAGER.record_syscall(id);
}

pub fn get_current_task_syscall_times() -> [usize; MAX_SYSCALL_NUM] {
    let si = TASK_MANAGER.current_task_syscall_times();
    si
}
