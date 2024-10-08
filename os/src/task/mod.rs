use context::TaskContext;
use lazy_static::lazy_static;
use switch::__switch;
use task::{TaskControlBlock, TaskStatus};

use crate::{config::MAX_APP_NUM, loader, sbi::shutdown, sync::UPSafeCell};

mod context;
mod switch;
mod task;

lazy_static! {
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = loader::get_num_app();
        let mut tasks = [TaskControlBlock {
            task_status: task::TaskStatus::UnInit,
            task_cx: TaskContext::zero_init(),
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
    tasks: [TaskControlBlock; MAX_APP_NUM],
    curr_task: usize,
}

impl TaskManager {
    fn run_first_task(&self) {
        let mut inner = self.inner.exclusive_access();

        let task0 = &mut inner.tasks[0];
        task0.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &task0.task_cx as *const TaskContext;
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
        inner.tasks[curr].task_status = TaskStatus::Ready;
    }

    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let curr = inner.curr_task;
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
                log::debug!("All applications completed!");
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
        unsafe {
            __switch(current_task_cx_ptr, next_task_cx_ptr);
        }
    }
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
