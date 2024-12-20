use crate::drivers::{CharDevice, UART};

use super::File;

///Standard input
pub struct Stdin;
///Standard output
pub struct Stdout;

impl File for Stdin {
    fn readable(&self) -> bool {
        true
    }

    fn writable(&self) -> bool {
        false
    }

    fn read(&self, mut user_buf: crate::mm::UserBuffer) -> usize {
        assert_eq!(user_buf.len(), 1);
        let ch = UART.read();
        unsafe {
            user_buf.buffers[0].as_mut_ptr().write_volatile(ch);
        }
        1
    }

    fn write(&self, _user_buf: crate::mm::UserBuffer) -> usize {
        panic!("Cannot write to stdin!");
    }
}

impl File for Stdout {
    fn readable(&self) -> bool {
        false
    }

    fn writable(&self) -> bool {
        true
    }

    fn read(&self, _user_buf: crate::mm::UserBuffer) -> usize {
        panic!("Cannot read from stdout!");
    }

    fn write(&self, user_buf: crate::mm::UserBuffer) -> usize {
        for buf in &user_buf.buffers {
            print!("{}", core::str::from_utf8(*buf).unwrap());
        }
        user_buf.len()
    }
}
