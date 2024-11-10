use core::panic::PanicInfo;

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
    shutdown(true)
}
