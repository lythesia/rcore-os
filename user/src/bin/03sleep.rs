#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{get_time, yield_};

#[no_mangle]
fn main() -> i32 {
    println!("sleep ..");
    let curr_timer = get_time();
    let wait_for = curr_timer + 3000; // sleep 3000ms
    while get_time() < wait_for {
        yield_(); // yield, gives 10ms slice to others
    }
    println!("Test sleep OK!");
    0
}
