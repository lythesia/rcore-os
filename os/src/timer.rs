use riscv::register::time;

use crate::config::CLOCK_FREQ;

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
