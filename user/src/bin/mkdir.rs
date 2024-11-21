#![no_std]
#![no_main]

use user_lib::{exit, mkdir};

#[macro_use]
extern crate user_lib;

#[no_mangle]
fn main(argc: usize, argv: &[&str]) -> i32 {
    assert_eq!(argc, 2, "wrong number of args!");
    let path = argv[1];

    if mkdir(path) == -1 {
        println!("Error mkdir {}", path);
        exit(-1);
    }
    0
}
