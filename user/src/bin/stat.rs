#![no_std]
#![no_main]

use user_lib::{exit, fstat, open, OpenFlags, Stat};

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
        println!("Error unlink {}", path);
        exit(-1);
    }

    println!("File:   {}", path);
    println!("Size:   {}\t{:?}", stat.size, stat.mode);
    println!("Device: {}", stat.dev);
    println!("Inode:  {}", stat.ino);
    0
}
