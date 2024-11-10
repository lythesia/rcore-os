use sbi_rt::{system_reset, NoReason, Shutdown, SystemFailure};

pub fn console_getchar() -> usize {
    #[allow(deprecated)]
    sbi_rt::legacy::console_getchar()
}

pub fn console_putchar(c: usize) {
    #[allow(deprecated)]
    sbi_rt::legacy::console_putchar(c);
}

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
