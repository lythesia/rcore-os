#![no_std]
#![no_main]

use user_lib::{close, exit, fork, open, read, waitpid, write, OpenFlags};

#[macro_use]
extern crate user_lib;

#[no_mangle]
pub fn main() -> i32 {
    let test_str = b"Hello, world!";
    let fname = "fname\0";
    let fd = open(fname, OpenFlags::CREATE | OpenFlags::WRONLY);
    assert!(fd > 0);
    let fd = fd as usize;
    write(fd, &test_str[..]);
    close(fd);

    let fd = open(fname, OpenFlags::RDONLY);
    assert!(fd > 0);
    let fd = fd as usize;
    let mut buffer = [0u8; 4];
    let read_len = read(fd, &mut buffer) as usize;
    println!(
        "main 1st read: {}",
        core::str::from_utf8(&buffer[..read_len]).unwrap()
    ); // should be "Hell"
    assert_eq!(&buffer[..read_len], &test_str[..read_len]);

    let pid = fork();
    if pid == 0 {
        buffer.fill(0);
        let n = read(fd, &mut buffer) as usize;
        println!(
            "child read: {}",
            core::str::from_utf8(&buffer[..n]).unwrap()
        ); // should be "o, w", coz inherit file cursor(offset)
        assert_eq!(&buffer[..n], &test_str[read_len..read_len + n]);
        close(fd);
        exit(0);
    } else {
        let mut exit_code = 0;
        waitpid(pid as usize, &mut exit_code);

        buffer.fill(0);
        let n = read(fd, &mut buffer) as usize;
        println!(
            "main 2nd read: {}",
            core::str::from_utf8(&buffer[..n]).unwrap()
        ); // should be also "o, w", coz file cursor(offset) independent
        assert_eq!(&buffer[..n], &test_str[read_len..read_len + n]);
    }
    0
}
