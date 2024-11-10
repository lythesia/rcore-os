#![no_std]
#![no_main]

#[no_mangle]
fn main() -> i32 {
    user_lib::halt();
    0
}
