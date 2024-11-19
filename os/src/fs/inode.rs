use alloc::{string::String, sync::Arc, vec::Vec};
use bitflags::bitflags;
use easy_fs::{EasyFileSystem, Inode};
use lazy_static::lazy_static;

use crate::{drivers::BLOCK_DEVICE, sync::UPSafeCell};

use super::File;

pub struct OSInode {
    readable: bool,
    writable: bool,
    inner: UPSafeCell<OSInodeInner>,
}

pub struct OSInodeInner {
    offset: usize,
    inode: Arc<Inode>,
}

impl OSInode {
    pub fn new(readable: bool, writable: bool, inode: Arc<Inode>) -> Self {
        Self {
            readable,
            writable,
            inner: unsafe { UPSafeCell::new(OSInodeInner { offset: 0, inode }) },
        }
    }

    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer = [0u8; 512];
        let mut v = Vec::new();
        loop {
            let len = inner.inode.read_at(inner.offset, &mut buffer);
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }

    pub fn is_dir(&self) -> bool {
        self.inner.exclusive_access().inode.is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.inner.exclusive_access().inode.is_file()
    }

    pub fn clone_inner_inode(&self) -> Arc<Inode> {
        self.inner.exclusive_access().inode.clone()
    }

    pub fn copy(&self) -> Self {
        let inner = self.inner.exclusive_access();
        Self {
            readable: self.readable,
            writable: self.writable,
            inner: unsafe {
                UPSafeCell::new(OSInodeInner {
                    offset: inner.offset,
                    inode: inner.inode.clone(),
                })
            },
        }
    }
}

lazy_static! {
    pub static ref ROOT_INODE: Arc<Inode> = {
        let efs = EasyFileSystem::open(BLOCK_DEVICE.clone());
        Arc::new(EasyFileSystem::root_inode(&efs))
    };
}

/// List all files in the filesystems
pub fn ls_root() {
    println!("/**** ls / ****");
    for app in ROOT_INODE.ls() {
        println!("{}", app);
    }
    println!("**************/");
}

bitflags! {
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RDRW = 1 << 1;
        const CREATE = 1 << 9;
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags {
    /// Do not check validity for simplicity
    /// Return (readable, writable)
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(OpenFlags::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

/// Open file with flags
pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    open_file_at(&ROOT_INODE, name, flags)
}

/// Open file relative to base
pub fn open_file_at(base: &Inode, name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();

    let (path, fname) = match name.rsplit_once('/') {
        Some(v) => v,
        _ => (".", name),
    };

    if flags.contains(OpenFlags::CREATE) {
        // find parent first
        match base.find(path) {
            Some(parent) => match parent.find(fname) {
                Some(inode) => {
                    if flags.contains(OpenFlags::TRUNC) {
                        inode.clear();
                    }
                    Some(Arc::new(OSInode::new(readable, writable, inode)))
                }
                _ => parent
                    .create(fname)
                    .map(|inode| Arc::new(OSInode::new(readable, writable, inode))),
            },
            _ => None,
        }
    } else {
        base.find(name).map(|inode| {
            if flags.contains(OpenFlags::TRUNC) {
                inode.clear();
            }
            Arc::new(OSInode::new(readable, writable, inode))
        })
    }
}

/// Unlink file relative to base TODO move to fs.rs?
pub fn unlink_file_at(base: &Inode, name: &str) -> bool {
    let (path, fname) = match name.rsplit_once('/') {
        Some(v) => v,
        _ => (".", name),
    };

    match base.find(path) {
        Some(parent) => parent.unlink(fname),
        _ => false,
    }
}

pub fn find_file(path: &str) -> Option<Arc<OSInode>> {
    assert!(path.starts_with('/'));
    ROOT_INODE
        .find(path)
        .map(|inode| Arc::new(OSInode::new(true, true, inode)))
}

impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }

    fn writable(&self) -> bool {
        self.writable
    }

    fn read(&self, mut buf: crate::mm::UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_read_size = 0;
        for slice in buf.buffers.iter_mut() {
            let len = inner.inode.read_at(inner.offset, *slice);
            if len == 0 {
                break;
            }
            inner.offset += len;
            total_read_size += len;
        }
        total_read_size
    }

    fn write(&self, buf: crate::mm::UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let len = inner.inode.write_at(inner.offset, *slice);
            assert_eq!(len, slice.len());
            inner.offset += len;
            total_write_size += len;
        }
        total_write_size
    }
}

pub fn name_of_inode(inode: &Inode, parent: &Inode) -> String {
    parent
        .read_dirent(inode.inode_id(), |d| String::from(d.name()))
        .expect("not exist in .. dir?!")
}

pub fn name_for_inode(inode: &Inode) -> String {
    fn inner(inode: &Inode) -> String {
        if inode.inode_id() == ROOT_INODE.inode_id() {
            return String::new();
        }

        let parent = inode.find("..").expect("parent `..' not exist?!");
        let name = name_of_inode(inode, &parent);
        let ancestor_name = inner(&parent);
        ancestor_name + "/" + &name
    }

    match inner(inode) {
        s if s.is_empty() => String::from("/"), // special case for ROOT
        s => s,
    }
}
