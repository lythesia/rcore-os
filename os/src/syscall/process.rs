use crate::{mm, task::*, timer};

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    log::debug!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// get time
pub fn sys_get_time(ts: *mut TimeVal) -> isize {
    let us = timer::get_time_us();
    let dst_vs = mm::translated_byte_buffer(
        current_user_token(),
        ts as *const u8,
        core::mem::size_of::<TimeVal>(),
    );
    let ts = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let ts_ptr = (&ts as *const TimeVal) as *const u8;
    for (i, dst) in dst_vs.into_iter().enumerate() {
        let len = dst.len();
        unsafe {
            let src = core::slice::from_raw_parts(ts_ptr.wrapping_add(i * len), len);
            dst.copy_from_slice(src);
        }
    }
    0
}
