use alloc::sync::Arc;
use bitflags::bitflags;
use easy_fs::Inode;

use crate::{
    cast::DowncastArc,
    fs::{self, name_for_inode, unlink_file_at, File, OSInode, OpenFlags, ROOT_INODE},
    mm::{self, translated_byte_buffer, UserBuffer},
    task::{self, TaskControlBlock},
};

macro_rules! bail_exit {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(exit) => return exit,
        }
    };
}

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

const AT_FDCWD: isize = -100;
/// determin base inode for *at_ series
/// 1. `abs_path`: works if it starts with "/"
/// 2. `open_read/write`: require the fd(dir) to be open with read/write
/// 3. `curr_task`: need access to TCB inner
fn base_inode(
    fd: isize,
    abs_path: &str,
    open_read: bool,
    open_write: bool,
    curr_task: &Arc<TaskControlBlock>,
) -> Result<Arc<Inode>, isize> {
    let base = match (abs_path.starts_with("/"), fd == AT_FDCWD) {
        (true, _) => ROOT_INODE.clone(),
        (_, true) => {
            // from cwd
            let cwd = curr_task.inner_exclusive_access().cwd.clone();
            let path = name_for_inode(&cwd);
            // ensure cwd
            match fs::find_file(&path) {
                Some(v) if v.is_dir() => cwd, // must be existed dir
                _ => return Err(-1),
            }
        }
        (_, false) => {
            // from fd specified, fd must be open
            match curr_task.inner_exclusive_access().fd_table.get(fd as usize) {
                Some(Some(file)) => {
                    let file_clone = file.clone();
                    match file_clone.downcast_arc::<OSInode>() {
                        Some(os_inode) if os_inode.is_dir() => {
                            if open_read && !os_inode.readable()
                                || open_write && !os_inode.writable()
                            {
                                return Err(-1); // caller should have rx on this dir if read or wx if write
                            }
                            os_inode.clone_inner_inode()
                        }
                        _ => return Err(-1), // not an efs dir
                    }
                }
                _ => return Err(-1),
            }
        }
    };
    Ok(base)
}

pub fn sys_openat(fd: isize, path: *const u8, flags: u32) -> isize {
    let open_flags = OpenFlags::from_bits_truncate(flags);
    let (or, ow) = open_flags.read_write();

    let curr_task = task::current_task().unwrap();
    let token = curr_task.inner_exclusive_access().get_user_token();
    let path = mm::translated_str(token, path);

    let base = bail_exit!(base_inode(fd, &path, or, ow, &curr_task));
    if let Some(inode) = fs::open_file_at(&base, &path, open_flags) {
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

pub fn sys_getcwd(ptr: *mut u8, len: usize) -> isize {
    let curr_task = task::current_task().unwrap();
    let task_inner = curr_task.inner_exclusive_access();
    let token = task_inner.get_user_token();

    let cwd = name_for_inode(&task_inner.cwd);
    if cwd.len() + 1 > len {
        return -1;
    }
    let src = cwd.as_bytes();

    let dst_vs = mm::translated_byte_buffer(token, ptr as *const u8, len);
    for (i, dst) in dst_vs.into_iter().enumerate() {
        let dst_len = dst.len().min(src.len());
        let s = i * dst_len;
        dst[..dst_len].copy_from_slice(&src[s..s + dst_len]);
    }
    0
}

pub fn sys_mkdirat(fd: isize, path: *const u8) -> isize {
    let curr_task = task::current_task().unwrap();
    let token = curr_task.inner_exclusive_access().get_user_token();
    let path = mm::translated_str(token, path);

    let mut base = bail_exit!(base_inode(fd, &path, true, true, &curr_task));
    for name in path.split("/").filter(|s| !s.is_empty()) {
        match base.create_dir(name) {
            Some(created) => base = created,
            // already exist
            _ => {
                // TODO can we avoid `find`?
                let existed = base.find(name).unwrap();
                if !existed.is_dir() {
                    return -1; // intermediate must be dir
                }
                base = existed;
            }
        }
    }
    0
}

pub fn sys_chdir(path: *const u8) -> isize {
    let curr_task = task::current_task().unwrap();
    let token = curr_task.inner_exclusive_access().get_user_token();
    let path = mm::translated_str(token, path);

    let base = bail_exit!(base_inode(AT_FDCWD, &path, true, true, &curr_task));
    match base.find(&path) {
        Some(d) => {
            if !d.is_dir() {
                return -1;
            }
            curr_task.inner_exclusive_access().cwd = d;
        }
        _ => {
            return -1;
        }
    }
    0
}

pub fn sys_unlinkat(fd: isize, path: *const u8) -> isize {
    // TODO support actual dirfd
    if fd != AT_FDCWD {
        return -1;
    }

    let curr_task = task::current_task().unwrap();
    let token = curr_task.inner_exclusive_access().get_user_token();
    let path = mm::translated_str(token, path);

    let base = bail_exit!(base_inode(AT_FDCWD, &path, true, true, &curr_task));
    if unlink_file_at(&base, &path) {
        0
    } else {
        -1
    }
}

pub fn sys_linkat(fd: isize, oldpath: *const u8, newpath: *const u8) -> isize {
    // TODO support actual dirfd
    if fd != AT_FDCWD {
        return -1;
    }

    let curr_task = task::current_task().unwrap();
    let token = curr_task.inner_exclusive_access().get_user_token();
    let oldpath = mm::translated_str(token, oldpath);
    let newpath = mm::translated_str(token, newpath);

    let oldbase = bail_exit!(base_inode(AT_FDCWD, &oldpath, true, true, &curr_task));
    let newbase = bail_exit!(base_inode(AT_FDCWD, &newpath, true, true, &curr_task));

    // parent.link(name, old_inode)
    // old must exist
    let old_inode = bail_exit!(oldbase.find(&oldpath).ok_or(-1));
    let (path, fname) = match newpath.rsplit_once('/') {
        Some(v) => v,
        _ => (".", newpath.as_str()),
    };
    // parent must exist
    let parent = bail_exit!(newbase.find(path).ok_or(-1));
    if parent.link(fname, &old_inode).is_some() {
        0
    } else {
        -1
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: StatMode,
    pub nlink: u32,
    pad: [u64; 7],
}

impl Stat {
    pub fn new(ino: u64, mode: StatMode, nlink: u32) -> Self {
        Self {
            dev: 0,
            ino,
            mode,
            nlink,
            pad: [0; 7],
        }
    }
}

bitflags! {
    pub struct StatMode: u32 {
        const NULL  = 0;
        /// directory
        const DIR   = 0o040000;
        /// ordinary regular file
        const FILE  = 0o100000;
    }
}

pub fn sys_fstat(fd: usize, ptr: *mut Stat) -> isize {
    let curr_task = task::current_task().unwrap();
    let task_inner = curr_task.inner_exclusive_access();

    // fd must exist
    let inode = match task_inner.fd_table.get(fd) {
        Some(Some(file)) => {
            let file_clone = file.clone();
            bail_exit!(file_clone.downcast_arc::<OSInode>().ok_or(-1)).clone_inner_inode()
        }
        _ => return -1,
    };

    let ino = inode.inode_id();
    let mode = if inode.is_dir() {
        StatMode::DIR
    } else if inode.is_file() {
        StatMode::FILE
    } else {
        StatMode::NULL
    };
    let nlink = inode.nlink();
    let stat = Stat::new(ino as u64, mode, nlink);

    let dst_vs = mm::translated_byte_buffer(
        task_inner.get_user_token(),
        ptr as *const u8,
        core::mem::size_of::<Stat>(),
    );
    let s_ptr = (&stat as *const Stat) as *const u8;
    for (i, dst) in dst_vs.into_iter().enumerate() {
        let len = dst.len();
        unsafe {
            let src = core::slice::from_raw_parts(s_ptr.wrapping_add(i * len), len);
            dst.copy_from_slice(src);
        }
    }
    0
}
