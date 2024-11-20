#![no_std]
#![no_main]

#[no_mangle]
fn main(_argc: usize, _argv: &[&str]) -> i32 {
    user_lib::halt();
    0
}
