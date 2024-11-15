use crate::{
    fs,
    mm::{self, translated_byte_buffer, UserBuffer},
    task,
};

/// write buf of length `len` to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let task = task::current_task().unwrap();
    let inner = task.inner_exclusive_access();
    let token = inner.get_user_token();

    match inner.fd_table.get(fd) {
        Some(Some(file)) => {
            if !file.writable() {
                return -1;
            }
            let file = file.clone();
            // release current task TCB manually to avoid multi-borrow
            drop(inner);
            file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
        }
        _ => -1,
    }
}

/// read buf of length `len` from a file with `fd`
pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let task = task::current_task().unwrap();
    let inner = task.inner_exclusive_access();
    let token = inner.get_user_token();

    match inner.fd_table.get(fd) {
        Some(Some(file)) => {
            if !file.readable() {
                return -1;
            }
            let file = file.clone();
            // release current task TCB manually to avoid multi-borrow
            drop(inner);
            file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
        }
        _ => -1,
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let curr_task = task::current_task().unwrap();
    let token = curr_task.inner_exclusive_access().get_user_token();
    let path = mm::translated_str(token, path);
    if let Some(inode) = fs::open_file(&path, fs::OpenFlags::from_bits_truncate(flags)) {
        let mut inner = curr_task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    let curr_task = task::current_task().unwrap();
    let mut inner = curr_task.inner_exclusive_access();
    if let Some(opt) = inner.fd_table.get_mut(fd) {
        match opt.take() {
            Some(_) => 0,
            _ => -1,
        }
    } else {
        -1
    }
}
