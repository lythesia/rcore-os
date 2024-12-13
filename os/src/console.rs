use crate::drivers::{CharDevice, UART};
use core::fmt::{self, Write};

struct Stdout;
impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            UART.write(c as u8);
        }
        Ok(())
    }
}

pub fn print(args: fmt::Arguments) {
    Stdout.write_fmt(args).unwrap();
}

// `tt` captures "," also
// so when $($arg:tt)+ matching { "rust_main", "os" }
// the tt stream will be: { token("rust_main"  token(,) token("os") }, 3 tokens
#[macro_export]
macro_rules! print {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::console::print(format_args!($fmt $(, $($arg)+)?));
    };
}

#[macro_export]
macro_rules! println {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::console::print(format_args!(concat!($fmt, "\n") $(, $($arg)+)?));
    }
}
