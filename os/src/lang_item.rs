use core::panic::PanicInfo;

use tracer::{FramePointTracer, Tracer};

use crate::sbi::shutdown;

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
    backtrace();
    shutdown(true)
}

fn backtrace() {
    println!("---START BACKTRACE---");
    let tracer = FramePointTracer::new(crate::trace::Provider);
    for v in tracer.trace() {
        println!("[{:#x}] (+{:0>4x}) {}", v.func_addr, v.bias, v.func_name);
    }
    println!("---END   BACKTRACE---");
}
