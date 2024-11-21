#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
extern crate alloc;

#[no_mangle]
fn main(_argc: usize, argv: &[&str]) -> i32 {
    let s = argv[1..].join(" ");
    println!("{}", s);
    0
}
