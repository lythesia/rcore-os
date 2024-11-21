#![no_std]
#![no_main]

use user_lib::{exit, fstat, open, unlink, OpenFlags, Stat, StatMode};

#[macro_use]
extern crate user_lib;

#[no_mangle]
fn main(argc: usize, argv: &[&str]) -> i32 {
    assert_eq!(argc, 2, "wrong number of args!");
    let path = argv[1];

    let fd = open(path, OpenFlags::RDONLY);
    if fd == -1 {
        println!("Error open {}", path);
        exit(-1);
    }
    let mut stat = Stat::new();
    if fstat(fd as usize, &mut stat) == -1 {
        println!("Error fstat {}", path);
        exit(-1);
    }

    const DIRENT_SZ: usize = 32;
    if stat.mode == StatMode::DIR && stat.size as usize > 2 * DIRENT_SZ {
        // 2 dirent: . and ..
        println!("Directory not empty!");
        exit(-1);
    }

    if unlink(path) == -1 {
        println!("Error unlink {}", path);
        exit(-1);
    }
    0
}
