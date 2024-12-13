use core::{arch::asm, panic::PanicInfo};

use crate::{sbi::shutdown, task::current_kstack_top};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        log::error!(
            "Panicked at {}:{} {}",
            loc.file(),
            loc.line(),
            info.message()
        );
    } else {
        log::error!("Panicked: {}", info.message());
    }
    unsafe {
        backtrace();
    }
    shutdown(true)
}

unsafe fn backtrace() {
    let mut fp: usize;
    let stop = current_kstack_top();
    asm!("mv {}, s0", out(reg) fp);
    println!("---START BACKTRACE---");
    for i in 0..10 {
        if fp == stop {
            break;
        }
        println!("#{}:ra={:#x}", i, *((fp - 8) as *const usize));
        fp = *((fp - 16) as *const usize);
    }
    println!("---END   BACKTRACE---");
}
