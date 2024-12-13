use alloc::sync::Arc;
use bitflags::bitflags;
use easy_fs::Inode;

use crate::{
    cast::DowncastArc,
    fs::{self, make_pipe, name_for_inode, unlink_file_at, File, OSInode, OpenFlags, ROOT_INODE},
    mm::{self, translated_byte_buffer, UserBuffer},
    task::{self, ProcessControlBlock},
};

use super::bail_exit;

/// write buf of length `len` to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let proc = task::current_process();
    let inner = proc.inner_exclusive_access();
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
    let proc = task::current_process();
    let inner = proc.inner_exclusive_access();
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
    curr_proc: &Arc<ProcessControlBlock>,
) -> Result<Arc<Inode>, isize> {
    let base = match (abs_path.starts_with("/"), fd == AT_FDCWD) {
        (true, _) => ROOT_INODE.clone(),
        (_, true) => {
            // from cwd
            let cwd = curr_proc.inner_exclusive_access().cwd.clone();
            let path = name_for_inode(&cwd);
            // ensure cwd
            match fs::find_file(&path) {
                Some(v) if v.is_dir() => cwd, // must be existed dir
                _ => return Err(-1),
            }
        }
        (_, false) => {
            // from fd specified, fd must be open
            match curr_proc.inner_exclusive_access().fd_table.get(fd as usize) {
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

    let proc = task::current_process();
    let token = proc.inner_exclusive_access().get_user_token();
    let path = mm::translated_str(token, path);

    let base = bail_exit!(base_inode(fd, &path, or, ow, &proc));
    if let Some(inode) = fs::open_file_at(&base, &path, open_flags) {
        let mut inner = proc.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    let proc = task::current_process();
    let mut inner = proc.inner_exclusive_access();
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
    let proc = task::current_process();
    let inner = proc.inner_exclusive_access();
    let token = inner.get_user_token();

    let cwd = name_for_inode(&inner.cwd);
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
    let proc = task::current_process();
    let token = proc.inner_exclusive_access().get_user_token();
    let path = mm::translated_str(token, path);

    let mut base = bail_exit!(base_inode(fd, &path, true, true, &proc));
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
    let curr_proc = task::current_process();
    let token = curr_proc.inner_exclusive_access().get_user_token();
    let path = mm::translated_str(token, path);

    let base = bail_exit!(base_inode(AT_FDCWD, &path, true, true, &curr_proc));
    match base.find(&path) {
        Some(d) => {
            if !d.is_dir() {
                return -1;
            }
            curr_proc.inner_exclusive_access().cwd = d;
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

    let curr_proc = task::current_process();
    let token = curr_proc.inner_exclusive_access().get_user_token();
    let path = mm::translated_str(token, path);

    let base = bail_exit!(base_inode(AT_FDCWD, &path, true, true, &curr_proc));
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

    let proc = task::current_process();
    let token = proc.inner_exclusive_access().get_user_token();
    let oldpath = mm::translated_str(token, oldpath);
    let newpath = mm::translated_str(token, newpath);

    let oldbase = bail_exit!(base_inode(AT_FDCWD, &oldpath, true, true, &proc));
    let newbase = bail_exit!(base_inode(AT_FDCWD, &newpath, true, true, &proc));

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
    pub size: u64, // added
    pad: [u64; 6],
}

impl Stat {
    pub fn new(ino: u64, mode: StatMode, nlink: u32, size: u64) -> Self {
        Self {
            dev: 0,
            ino,
            mode,
            nlink,
            size,
            pad: [0; 6],
        }
    }
}

bitflags! {
    #[derive(Default)]
    pub struct StatMode: u32 {
        const NULL  = 0;
        /// directory
        const DIR   = 0o040000;
        /// ordinary regular file
        const FILE  = 0o100000;
    }
}

pub fn sys_fstat(fd: usize, ptr: *mut Stat) -> isize {
    let proc = task::current_process();
    let task_inner = proc.inner_exclusive_access();

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
    let size = inode.get_size();
    let nlink = inode.nlink();
    let stat = Stat::new(ino as u64, mode, nlink, size as u64);

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

pub fn sys_pipe(pipe: *mut usize) -> isize {
    let proc = task::current_process();
    let mut inner = proc.inner_exclusive_access();
    let token = inner.get_user_token();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(pipe_read);
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(pipe_write);
    *mm::translated_refmut(token, pipe) = read_fd;
    *mm::translated_refmut(token, unsafe { pipe.add(1) }) = write_fd;
    0
}

pub fn sys_dup(fd: usize) -> isize {
    let proc = task::current_process();
    let mut inner = proc.inner_exclusive_access();
    let file = match inner.fd_table.get(fd) {
        Some(Some(file)) => file.clone(),
        _ => return -1,
    };
    let new_fd = inner.alloc_fd();
    inner.fd_table[new_fd] = Some(file);
    new_fd as isize
}

/// The max length of inode name
const NAME_LENGTH_LIMIT: usize = 27;
#[repr(C, align(32))]
#[derive(Clone, Default)]
pub struct Dirent {
    pub ftype: FileType,
    pub name: [u8; NAME_LENGTH_LIMIT],
    pub next_offset: u32,
}

bitflags! {
    #[derive(Default)]
    pub struct FileType: u8 {
        const UNKNOWN = 0;
        const DIR = 1 << 0;
        const REG = 1 << 1;
    }
}

pub fn sys_getdents(fd: usize, ptr: *mut Dirent, len: usize) -> isize {
    let proc = task::current_process();
    let inner = proc.inner_exclusive_access();
    let token = inner.get_user_token();

    let file = match inner.fd_table.get(fd) {
        Some(Some(file)) => {
            let file_clone = file.clone();
            match file_clone.downcast_arc::<OSInode>() {
                Some(os_inode) => os_inode.clone_inner_inode(),
                _ => return -1,
            }
        }
        _ => return -1,
    };
    drop(inner); // MUST drop here, coz `file.dirents` causes block read, when non-blocking, it'll schedule out w/ RefMut held!
    if !file.is_dir() {
        return -1;
    }

    let cursor = mm::translate_ref(token, unsafe { ptr.add(len - 1) }).next_offset;
    let dirents = file.dirents(cursor);
    let nread = len.min(dirents.len());
    for i in 0..nread {
        let ename = dirents[i].0.as_bytes();
        let inode = &dirents[i].1;
        let ftype = if inode.is_dir() {
            FileType::DIR
        } else if inode.is_file() {
            FileType::REG
        } else {
            FileType::UNKNOWN
        };
        let mut name = [0u8; NAME_LENGTH_LIMIT];
        name[..ename.len()].copy_from_slice(&ename[..]);
        *mm::translated_refmut(token, unsafe { ptr.add(i) }) = Dirent {
            ftype,
            name,
            next_offset: cursor + i as u32 + 1,
        };
    }
    nread as isize
}
