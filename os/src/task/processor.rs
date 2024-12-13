use core::arch::asm;

use alloc::sync::Arc;
use lazy_static::lazy_static;

use crate::{sync::UPIntrFreeCell, trap::TrapContext};

use super::{
    context::TaskContext,
    manager,
    switch::__switch,
    task::{TaskControlBlock, TaskStatus},
    ProcessControlBlock,
};

lazy_static! {
    // 在单核CPU环境下, 我们仅创建单个 Processor 的全局实例
    pub static ref PROCESSOR: UPIntrFreeCell<Processor> = unsafe { UPIntrFreeCell::new(Processor::new()) };
}

pub struct Processor {
    /// 当前处理器上正在执行的任务
    current: Option<Arc<TaskControlBlock>>,
    /// Processor 有一个不同的 idle 控制流, 它运行在这个 CPU 核的启动栈上,
    /// 功能是尝试从任务管理器中选出一个任务来在当前 CPU 核上执行。
    /// 在内核初始化完毕之后, 会通过调用 `run_tasks` 函数来进入 idle 控制流
    idle_task_cx: TaskContext,

    /// 停表
    stop_watch: usize,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
            stop_watch: 0,
        }
    }

    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }

    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }

    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut TaskContext
    }

    /// stop_watch <- now, return time of `last stop` until `now`
    fn refresh_stop_watch(&mut self) -> usize {
        let start = self.stop_watch;
        self.stop_watch = crate::timer::get_time_us();
        self.stop_watch - start
    }
}

pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

pub fn current_process() -> Arc<ProcessControlBlock> {
    current_task().unwrap().process.upgrade().unwrap()
}

pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    task.get_user_token()
}

pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

pub fn current_trap_cx_user_va() -> usize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .trap_cx_user_va()
}

#[allow(unused)]
pub fn current_kstack_top() -> usize {
    if let Some(task) = current_task() {
        task.kstack.get_top()
    } else {
        let mut boot_stack_top;
        unsafe { asm!("la {},boot_stack_top",out(reg) boot_stack_top) };
        boot_stack_top
    }
}

/// 从 idle 控制流通过任务调度切换到某个任务开始执行
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = manager::fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let next_task_cx_ptr = task.inner.exclusive_session(|task_inner| {
                task_inner.task_status = TaskStatus::Running;
                &task_inner.task_cx as *const TaskContext
            });
            // Arc<TaskControlBlock> 形式的任务从TaskManager流动到了处Processor
            processor.current = Some(task);
            // 开始记录时间
            processor.refresh_stop_watch();
            // stop exclusively accessing processor manually
            drop(processor);

            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            // no available task
        }
    }
}

/// 当一个应用用尽了内核本轮分配给它的时间片或者它主动调用 yield 后, 内核会调用 `schedule` 函数来切换到 idle 控制流并开启新一轮的任务调度
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let idle_task_cx_ptr =
        PROCESSOR.exclusive_session(|processor| processor.get_idle_task_cx_ptr());
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}

/// stop_watch <- now, return time of `last stop` until `now`
#[allow(unused)]
pub fn refresh_stop_watch() -> usize {
    PROCESSOR.exclusive_access().refresh_stop_watch()
}

pub fn user_time_start() {
    // let process = current_process();
    // let mut inner = process.inner_exclusive_access();
    // // 到user_time_start为止都是kernel_time, 故累加
    // // 隐含另一个意思, 从现在开始是user_time
    // inner.kernel_time += refresh_stop_watch();
}

pub fn user_time_end() {
    // let process = current_process();
    // let mut inner = process.inner_exclusive_access();
    // // 类似上面, 到user_time_end为止都是user_time, 故累加
    // // 隐含另一个意思, 从现在开始是kernel_time
    // inner.user_time += refresh_stop_watch();
}
