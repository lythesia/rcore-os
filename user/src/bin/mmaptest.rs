#![no_std]
#![no_main]

use user_lib::{close, exit, fork, mmap, munmap, open, read, waitpid, write, MMapFlags, OpenFlags};

#[macro_use]
extern crate user_lib;

#[no_mangle]
fn main() -> i32 {
    // anon_0();
    // anon_1();
    // anon_2();
    // anon_3();
    // anon_4();
    // anon_5();
    // anon_6();
    // file_0();
    // file_1();
    file_2();
    // mix_0();
    0
}

fn mmap_anon(start: usize, len: usize, prot: usize) -> isize {
    let flags = if start != 0 {
        MMapFlags::MAP_ANON | MMapFlags::MAP_FIXED
    } else {
        MMapFlags::MAP_ANON
    };
    mmap(start, len, prot, flags, 0, 0)
}

#[allow(unused)]
fn anon_0() {
    let start: usize = 0x10000000;
    let len: usize = 4096;
    let prot: usize = 3; // wr
    println!("case anon: start={:#x} len={} prot={:#b}", start, len, prot);
    let ret = mmap_anon(start, len, prot);
    assert_ne!(-1, ret);
    assert_eq!(start, ret as usize);

    for i in start..(start + len) {
        let addr: *mut u8 = i as *mut u8;
        unsafe {
            *addr = i as u8;
        }
    }
    for i in start..(start + len) {
        let addr: *mut u8 = i as *mut u8;
        unsafe {
            assert_eq!(*addr, i as u8);
        }
    }
    println!("anon_0 OK!");
}

#[allow(unused)]
fn anon_1() {
    let start: usize = 0x10000000;
    let len: usize = 4096;
    let prot: usize = 1; // r
    println!("case anon: start={:#x} len={} prot={:#b}", start, len, prot);
    let ret = mmap_anon(start, len, prot);
    assert_ne!(-1, ret);
    assert_eq!(start, ret as usize);
    let addr: *mut u8 = start as *mut u8;
    unsafe {
        *addr = start as u8;
    }
    println!("anon_1 Should cause error!");
}

#[allow(unused)]
fn anon_2() {
    let start: usize = 0x10000000;
    let len: usize = 4096;
    let prot: usize = 2; // w
    println!("case anon: start={:#x} len={} prot={:#b}", start, len, prot);
    let ret = mmap_anon(start, len, prot);
    assert_ne!(-1, ret);
    assert_eq!(start, ret as usize);
    let addr: *mut u8 = start as *mut u8;
    unsafe {
        *addr = start as u8; // can't write, R == 0 && W == 1 is illegal in riscv
        assert!(*addr != 0);
    }
    println!("anon_2 Should cause error!");
}

#[allow(unused)]
fn anon_3() {
    let start: usize = 0x10000000;
    let len: usize = 4096;
    let prot: usize = 3; // wr
    println!("case anon: start={:#x} len={} prot={:#b}", start, len, prot);
    let ret = mmap_anon(start, len, prot);
    assert_ne!(-1, ret);
    assert_eq!(start, ret as usize);

    assert_eq!(mmap_anon(start - len, len + 1, prot), -1); // overlap
    assert_eq!(mmap_anon(start + len + 1, len, prot), -1); // start not aligned
    assert_eq!(mmap_anon(start + len, len, 0), -1); // invalid prot: no perm specified
    assert_eq!(mmap_anon(start + len, len, prot | 8), -1); // invalid prot: only last 3 bits allowed
    println!("anon_3 OK!");
}

#[allow(unused)]
fn anon_4() {
    let start: usize = 0x10000000;
    let len: usize = 4096;
    let prot: usize = 3; // wr
    println!("case anon: start={:#x} len={} prot={:#b}", start, len, prot);
    let ret = mmap_anon(start, len, prot);
    assert_ne!(-1, ret);
    assert_eq!(start, ret as usize);

    let ret2 = mmap_anon(start + len, len * 2, prot);
    assert_ne!(-1, ret2);
    assert_eq!(ret2 as usize, start + len);
    assert_eq!(munmap(start, len), 0); // unmap 1st

    let ret3 = mmap_anon(start - len, len + 1, prot); // able to map though overlap with 1st
    assert_ne!(-1, ret3);
    assert_eq!(ret3 as usize, start - len);
    for i in (start - len)..(start + len * 3) {
        let addr: *mut u8 = i as *mut u8;
        unsafe {
            *addr = i as u8;
        }
    }
    for i in (start - len)..(start + len * 3) {
        let addr: *mut u8 = i as *mut u8;
        unsafe {
            assert_eq!(*addr, i as u8);
        }
    }
    println!("anon_4 OK!");
}

#[allow(unused)]
fn anon_5() {
    let start: usize = 0x10000000;
    let len: usize = 4096;
    let prot: usize = 3; // wr
    println!("case anon: start={:#x} len={} prot={:#b}", start, len, prot);
    let ret = mmap_anon(start, len, prot);
    assert_ne!(-1, ret);
    assert_eq!(start, ret as usize);

    assert_eq!(munmap(start, len + 1), -1); // len exceed
    assert_eq!(munmap(start + 1, len - 1), -1); // start not aligned
    println!("anon_5 OK!");
}

#[allow(unused)]
fn anon_6() {
    let len: usize = 4096;
    let prot: usize = 3;
    println!("case anon: start=? len={} prot={:#b}", len, prot);
    let ret = mmap_anon(0, len, prot);
    assert_ne!(-1, ret);
    let start = ret as usize;
    println!("start={:#x}", start);

    for i in start..(start + len) {
        let addr: *mut u8 = i as *mut u8;
        unsafe {
            *addr = i as u8;
        }
    }
    for i in start..(start + len) {
        let addr: *mut u8 = i as *mut u8;
        unsafe {
            assert_eq!(*addr, i as u8);
        }
    }
    assert_eq!(munmap(start, len + 1), -1); // len exceed
    assert_eq!(munmap(start + 1, len - 1), -1); // start not aligned
    assert_eq!(munmap(start, len), 0); // unmap 1st

    println!("start={:#x} len={} prot={:#b}", start - len, len + 1, prot);
    let ret2 = mmap_anon(start - len, len + 1, prot); // able to map though overlap with 1st
    assert_ne!(-1, ret2);
    assert_eq!(ret2 as usize, start - len);
    for i in (start - len)..(start + 1) {
        let addr: *mut u8 = i as *mut u8;
        unsafe {
            *addr = i as u8;
        }
    }
    for i in (start - len)..(start + 1) {
        let addr: *mut u8 = i as *mut u8;
        unsafe {
            assert_eq!(*addr, i as u8);
        }
    }
    println!("anon_6 OK!");
}

#[allow(unused)]
fn file_0() {
    let fd = open(
        "file_0.txt\0", // file MUST end with \0
        OpenFlags::RDRW | OpenFlags::CREATE | OpenFlags::TRUNC,
    );
    assert!(fd > 0);
    let fd = fd as usize;
    let str = b"asdbasdq3423423\n";
    let len = str.len();
    assert_eq!(write(fd, &str[..]) as usize, len);

    let prot = 3;
    println!("case file: start=? len={} prot={:#b} fd={}", len, prot, fd);
    let ret1 = mmap(0, len, prot, MMapFlags::MAP_FILE, fd, 0);
    assert_ne!(ret1, -1);
    let p1 = unsafe { core::slice::from_raw_parts_mut(ret1 as usize as *mut u8, len) };
    let ret2 = mmap(0, len, prot, MMapFlags::MAP_FILE, fd, 0);
    assert_ne!(ret2, -1);
    let p2 = unsafe { core::slice::from_raw_parts_mut(ret2 as usize as *mut u8, len) };
    println!(
        "p1 = {:#x}, p2 = {:#x}",
        p1.as_mut_ptr() as usize,
        p2.as_mut_ptr() as usize
    );
    assert_ne!(close(fd), -1);

    p1[1] = '1' as u8;
    p2[2] = '2' as u8;
    p2[0] = '2' as u8;
    p1[0] = '1' as u8;

    println!("content1: {}", core::str::from_utf8(p1).unwrap()); // content1: 112basdq3423423
    println!("content2: {}", core::str::from_utf8(p2).unwrap()); // content2: 112basdq3423423

    let fd = open("2.txt\0", OpenFlags::RDONLY);
    assert!(fd > 0);
    let fd = fd as usize;
    let mut buf = [0; 16];
    assert_eq!(read(fd, &mut buf[..]) as usize, len);
    assert_eq!(str, &buf); // NOT sync
    assert_ne!(close(fd), -1);
}

#[allow(unused)]
fn file_1() {
    let fd = open(
        "file_1.txt\0",
        OpenFlags::RDRW | OpenFlags::CREATE | OpenFlags::TRUNC,
    );
    assert!(fd > 0);
    let fd = fd as usize;
    let str = b"thisisfile\n";
    let len = str.len();
    assert_eq!(write(fd, &str[..]) as usize, len);

    let start: usize = 0x10000000;
    let prot = 3;
    println!(
        "case file: start={:#x} len={} prot={:#b} fd={}",
        start, len, prot, fd
    );
    assert_ne!(
        mmap(
            start,
            len,
            prot,
            MMapFlags::MAP_FILE | MMapFlags::MAP_FIXED,
            fd,
            0
        ),
        -1
    );
    assert_ne!(close(fd), -1);
    let p = unsafe { core::slice::from_raw_parts_mut(start as *mut u8, len) };
    p[0] = 'X' as u8;
    println!("main munmap");
    assert_eq!(munmap(start, len), 0); // sync writes on munmap

    let pid = fork();
    if pid == 0 {
        let prot = 1;
        let fd = open("mix_0.txt\0", OpenFlags::RDONLY);
        assert!(fd > 0);
        let fd = fd as usize;
        println!(
            "child: start={:#x} len={} prot={:#b} fd={}",
            start, len, prot, fd
        );
        assert_ne!(
            mmap(
                start,
                len,
                prot,
                MMapFlags::MAP_FILE | MMapFlags::MAP_FIXED,
                fd,
                0
            ),
            -1
        ); // mmap in child process
        assert_ne!(close(fd), -1);
        let p = unsafe { core::slice::from_raw_parts(start as *mut u8, len) };
        assert_eq!(p[0] as char, 'X'); // able to read latest content
        exit(0);
    } else {
        let mut child_exit = 0;
        if waitpid(pid as usize, &mut child_exit) < 0 {
            println!("wait {} fail", pid);
        }
    }
}

#[allow(unused)]
fn file_2() {
    let fd = open(
        "file_2.txt\0",
        OpenFlags::RDRW | OpenFlags::CREATE | OpenFlags::TRUNC,
    );
    assert!(fd > 0);
    let fd = fd as usize;
    let str = b"thisisfile\n";
    let len = str.len();
    assert_eq!(write(fd, &str[..]) as usize, len);

    let start: usize = 0x10000000;
    let prot = 3;
    println!(
        "case file: start={:#x} len={} prot={:#b} fd={}",
        start, len, prot, fd
    );
    assert_ne!(
        mmap(
            start,
            len,
            prot,
            MMapFlags::MAP_FILE | MMapFlags::MAP_FIXED,
            fd,
            0
        ),
        -1
    );
    assert_ne!(close(fd), -1);
    let p = unsafe { core::slice::from_raw_parts_mut(start as *mut u8, len) };
    p[0] = 'X' as u8;

    let pid = fork();
    if pid == 0 {
        let p = unsafe { core::slice::from_raw_parts(start as *const u8, len) }; // ok to access mmap area
        println!("child: {}", core::str::from_utf8(p).unwrap());
        exit(0);
    } else {
        let mut child_exit = 0;
        if waitpid(pid as usize, &mut child_exit) < 0 {
            println!("wait {} fail", pid);
        }
    }
}

#[allow(unused)]
fn mix_0() {
    let fd = open(
        "mix_0.txt\0", // file MUST end with \0
        OpenFlags::RDRW | OpenFlags::CREATE | OpenFlags::TRUNC,
    );
    assert!(fd > 0);
    let fd = fd as usize;
    let str = b"thisisfile\n";
    let len = str.len();
    assert_eq!(write(fd, &str[..]) as usize, len);

    let start: usize = 0x10000000;
    let prot = 3;
    println!(
        "case file: start={:#x} len={} prot={:#b} fd={}",
        start, len, prot, fd
    );
    let ret = mmap(
        0,
        len,
        prot,
        MMapFlags::MAP_FILE | MMapFlags::MAP_FIXED,
        fd,
        0,
    );
    assert_ne!(ret, -1);
    println!("ret = {:#x}", ret as usize);
    assert_ne!(close(fd), -1);

    assert_eq!(mmap_anon(start - len, len + 1, prot), -1);
}
