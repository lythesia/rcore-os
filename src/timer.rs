use riscv::register::time;

use crate::config::CLOCK_FREQ;

const MICRO_PER_SEC: usize = 1_000_000;
pub fn sleep_us(us: usize) {
    let start = time::read(); // mtime ticks CLOCK_FREQ/sec
    while time::read() - start < us * (CLOCK_FREQ / MICRO_PER_SEC) {}
}
