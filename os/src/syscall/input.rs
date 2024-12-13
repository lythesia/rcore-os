use crate::drivers::KEYBOARD_DEVICE;

pub fn sys_event_get() -> isize {
    let kb = KEYBOARD_DEVICE.clone();
    if !kb.is_empty() {
        kb.read_event() as isize
    } else {
        0
    }
}

use crate::drivers::chardev::UART;

/// check UART's read-buffer is empty or not
pub fn sys_key_pressed() -> isize {
    let res = !UART.read_buffer_is_empty();
    if res {
        1
    } else {
        0
    }
}
