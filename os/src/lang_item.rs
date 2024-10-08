use core::panic::PanicInfo;

use crate::{sbi::shutdown, stack_trace::print_stack_trace};

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
        print_stack_trace();
    }

    shutdown(true)
}
