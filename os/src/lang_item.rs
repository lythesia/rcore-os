use core::panic::PanicInfo;

use crate::{println, sbi::shutdown};

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
    shutdown(true)
}
