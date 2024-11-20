#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{exec, fork, getpid, wait, yield_};

#[no_mangle]
fn main(_argc: usize, _argv: &[&str]) -> i32 {
    println!("initproc! pid = {}", getpid());
    if fork() == 0 {
        exec("user_shell\0", &[core::ptr::null::<u8>()]);
    } else {
        loop {
            let mut exit_code: i32 = 0;
            let pid = wait(&mut exit_code);
            if pid == -1 {
                yield_();
                continue;
            }
        }
    }
    0
}
