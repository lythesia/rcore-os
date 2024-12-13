use sbi_rt::{system_reset, NoReason, Shutdown, SystemFailure};

pub fn set_timer(timer: usize) {
    sbi_rt::set_timer(timer as u64);
}

pub fn shutdown(failure: bool) -> ! {
    if !failure {
        system_reset(Shutdown, NoReason);
    } else {
        system_reset(Shutdown, SystemFailure);
    }
    unreachable!()
}
