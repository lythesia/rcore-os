#![no_std]
#![no_main]

use user_lib::{
    chdir, close, exit, fork, getcwd, mkdir, mkdirat, open, openat, read, waitpid, write, OpenFlags,
};

#[macro_use]
extern crate user_lib;

#[no_mangle]
fn main() -> i32 {
    pwd();

    println!("/: mkdir & chdir: foo");
    mkdir("foo\0");
    chdir("foo\0");
    pwd();

    println!("/foo: mkdir & chdir: bar/baz");
    mkdir("bar/baz\0");
    chdir("bar/baz\0");
    pwd();

    println!("/foo/bar/baz: chdir: /foo");
    chdir("/foo\0");
    pwd();

    println!("/foo: mkdir & chdir: /foo/bar/baz0\0");
    mkdir("/foo/bar/baz0\0");
    chdir("/foo/bar/baz0\0");
    pwd();

    println!("/foo/bar/baz0: chdir ..");
    chdir("..\0");
    pwd(); // "/foo/bar"

    match fork() {
        0 => {
            println!("child pwd");
            pwd(); // "/foo/bar"

            println!("/foo/bar: chdir: /");
            chdir("/\0");
            pwd(); // "/"
            println!("child exit");
            exit(0);
        }
        pid => {
            let mut exit_code = 0;
            waitpid(pid as usize, &mut exit_code);
        }
    }

    println!("back to main");
    pwd();
    println!("open '.'");
    let fd = open(".\0", OpenFlags::RDRW); // "/foo/bar"
    assert_ne!(fd, -1);
    println!("openat('.', 'f.txt') & wr");
    let file = openat(
        fd as usize,
        "f.txt\0",
        OpenFlags::CREATE | OpenFlags::WRONLY,
    );
    assert_ne!(file, -1);
    write(file as usize, b"write to openat");
    close(file as usize);
    let file = open("/foo/bar/f.txt\0", OpenFlags::RDONLY);
    assert_ne!(file, -1);
    let mut buf = [0; 16];
    let n = read(file as usize, &mut buf) as usize;
    println!("read #{}: {}", n, core::str::from_utf8(&buf[..n]).unwrap());
    close(file as usize);

    println!("mkdirat('.', 'j/k') & chdir");
    mkdirat(fd as usize, "j/k\0");
    chdir("j/k\0");
    pwd();

    println!("mkdirat(abs) '/jk/jk'");
    mkdirat(fd as usize, "/jk/jk\0");

    println!("openat(abs) '/jk/jk/f.txt' & w");
    let file = openat(
        fd as usize,
        "/jk/jk/f.txt\0",
        OpenFlags::CREATE | OpenFlags::WRONLY,
    );
    assert_ne!(file, -1);
    let n = write(file as usize, b"write to openat(abs)");
    assert_ne!(n, -1);
    println!("write #{}", n);
    close(file as usize);

    println!("chdir /jk/jk");
    chdir("/jk/jk\0");
    pwd();
    println!("open f.txt");
    let file = open("f.txt\0", OpenFlags::RDONLY);
    assert_ne!(file, -1);
    let mut buf = [0; 32];
    let n = read(file as usize, &mut buf) as usize;
    println!("read #{}: {}", n, core::str::from_utf8(&buf[..n]).unwrap());
    close(file as usize);
    0
}

fn pwd() {
    let mut path_buf = [0u8; 128];
    getcwd(&mut path_buf[..]);
    let path = core::ffi::CStr::from_bytes_until_nul(&path_buf[..]).unwrap();
    println!("pwd: {:?}", path);
}
