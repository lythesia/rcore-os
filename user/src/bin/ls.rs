#![no_std]
#![no_main]

use alloc::vec;
use user_lib::{exit, getdents, open, Dirent, FileType, OpenFlags};

#[macro_use]
extern crate user_lib;
extern crate alloc;

#[no_mangle]
fn main(argc: usize, argv: &[&str]) -> i32 {
    let path = if argc == 1 {
        ".\0"
    } else if argc == 2 {
        argv[1]
    } else {
        panic!("wrong number of args!");
    };
    let fd = open(path, OpenFlags::RDONLY);
    if fd == -1 {
        println!("Error open {}", path);
        exit(-1);
    }

    const BUF_SIZE: usize = 16;
    let mut total = 0usize;
    let mut entries = vec![Dirent::default(); BUF_SIZE];
    let mut n = BUF_SIZE;
    loop {
        n = match getdents(fd as usize, &mut entries.as_mut_slice()[..n]) {
            -1 => {
                println!("Error read dir {}", path);
                exit(-1)
            }
            0 => break,
            v => v as usize,
        };
        total += n;
        for i in 0..n {
            let entry = &entries[i];
            let color_code = match entry.ftype {
                FileType::DIR => 94,
                FileType::REG => 0,
                _ => panic!("unknown file type {}", entry.name()),
            };
            print_color(format_args!("{}\n", entry.name()), color_code);
        }
    }
    print_color(format_args!("total {}\n", total), 90);
    0
}

fn print_color(args: core::fmt::Arguments, color_code: u8) {
    print!("\u{1b}[{}m{}\u{1b}[0m", color_code, args);
}
