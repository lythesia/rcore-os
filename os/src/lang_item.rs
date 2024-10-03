use core::panic::PanicInfo;

use crate::{println, sbi::shutdown};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        println!(
            "panicked at {}:{} {}",
            loc.file(),
            loc.line(),
            info.message()
        );
    } else {
        println!("panicked: {}", info.message());
    }
    shutdown(true)
}
