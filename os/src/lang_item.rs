use core::panic::PanicInfo;

use crate::{println, sbi::shutdown, stack_trace::print_stack_trace};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        println!(
            "Panicked at {}:{} {}",
            loc.file(),
            loc.line(),
            info.message()
        );
    } else {
        println!("Panicked: {}", info.message());
    }

    unsafe {
        print_stack_trace();
    }

    shutdown(true)
}
