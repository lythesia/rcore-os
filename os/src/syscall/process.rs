use crate::task::*;

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    log::debug!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}
