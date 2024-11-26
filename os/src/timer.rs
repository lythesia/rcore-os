use alloc::{collections::binary_heap::BinaryHeap, sync::Arc};
use core::cmp::Ordering;
use lazy_static::lazy_static;
use riscv::register::time;

use crate::{
    config::CLOCK_FREQ,
    sync::UPSafeCell,
    task::{self, TaskControlBlock},
};

const MS_PER_SEC: usize = 1000;
const US_PER_SEC: usize = 1_000_000;
const TICKS_PER_SEC: usize = 100; // 10ms/tick

/// read the `mtime` register
pub fn get_time() -> usize {
    time::read()
}

/// get current time in ms
#[allow(unused)]
pub fn get_time_ms() -> usize {
    time::read() / (CLOCK_FREQ / MS_PER_SEC)
}

/// get current time in us
pub fn get_time_us() -> usize {
    time::read() / (CLOCK_FREQ / US_PER_SEC)
}

/// set the next timer interrupt
pub fn set_next_trigger() {
    crate::sbi::set_timer(get_time() + CLOCK_FREQ / TICKS_PER_SEC);
}

lazy_static! {
    static ref TIMERS: UPSafeCell<BinaryHeap<TimerCondVar>> =
        unsafe { UPSafeCell::new(BinaryHeap::<TimerCondVar>::new()) };
}

pub struct TimerCondVar {
    pub expire_ms: usize,
    pub task: Arc<TaskControlBlock>,
}
impl PartialEq for TimerCondVar {
    fn eq(&self, other: &Self) -> bool {
        self.expire_ms == other.expire_ms
    }
}
impl Eq for TimerCondVar {}
impl PartialOrd for TimerCondVar {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let a = -(self.expire_ms as isize);
        let b = -(other.expire_ms as isize);
        Some(a.cmp(&b))
    }
}
impl Ord for TimerCondVar {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

pub fn add_timer(expire_ms: usize, task: Arc<TaskControlBlock>) {
    let mut timers = TIMERS.exclusive_access();
    timers.push(TimerCondVar { expire_ms, task });
}

pub fn check_timer() {
    let current_ms = get_time_ms();
    let mut timers = TIMERS.exclusive_access();
    while let Some(timer) = timers.peek() {
        if timer.expire_ms <= current_ms {
            // wakeup task
            task::wakeup_task(timer.task.clone());
            timers.pop();
        } else {
            // stop early coz heap is ordered
            break;
        }
    }
}

pub fn remove_timer(task: &Arc<TaskControlBlock>) {
    let mut timers = TIMERS.exclusive_access();
    let rm_ptr = Arc::as_ptr(&task);
    timers.retain(|v| Arc::as_ptr(&v.task) != rm_ptr);
}
