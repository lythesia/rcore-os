use crate::{task::*, timer::get_time_ms};

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

/// get time in ms
pub fn sys_get_time() -> isize {
    get_time_ms() as isize
}

// /// get info of current task
// pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
//     unsafe {
//         *ti = TaskInfo {
//             status: get_current_task_status(),
//             call: get_current_task_syscall_times(),
//             time: get_current_task_run_time(),
//         }
//     }
//     0
// }
