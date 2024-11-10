#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user_lib;

use alloc::{borrow::ToOwned, string::String};
use user_lib::{console::getchar, exec, fork, waitpid};

const BS: u8 = 0x08;
const LF: u8 = 0x0a;
const CR: u8 = 0x0d;
const DEL: u8 = 0x7f;

#[no_mangle]
fn main() -> i32 {
    println!("Rust user shell");
    let mut line: String = String::new();
    print!(">> ");
    loop {
        let c = getchar();
        match c {
            LF | CR => {
                println!("");
                let input = line.trim();
                if input.is_empty() {
                    line.clear();
                    print!(">> ");
                    continue;
                }
                // fork & exec
                let pid = fork();
                if pid == 0 {
                    // child process
                    let mut prog = input.to_owned();
                    prog.push('\0');
                    if exec(&prog) == -1 {
                        println!("[shell] cannot exec: {}", prog);
                        return -4;
                    }
                    unreachable!()
                } else {
                    // current process
                    let mut exit_code = 0;
                    let exit_pid = waitpid(pid as usize, &mut exit_code);
                    assert_eq!(exit_pid, pid);
                    println!("[shell] Process: pid={} exit_code={}", pid, exit_code);
                }
                line.clear();
                print!(">> ");
            }
            BS | DEL => {
                if !line.is_empty() {
                    print!("{}", BS as char);
                    print!(" ");
                    print!("{}", BS as char);
                    line.pop();
                }
            }
            _ => {
                print!("{}", c as char);
                line.push(c as char);
            }
        }
    }
}
