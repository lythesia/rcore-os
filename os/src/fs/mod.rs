use core::any::Any;

use crate::{cast::DowncastArc, mm::UserBuffer};

mod inode;
mod stdio;
pub use inode::{list_apps, open_file, OSInode, OpenFlags};
pub use stdio::{Stdin, Stdout};

pub trait File: Any + Send + Sync {
    /// If readable
    fn readable(&self) -> bool;
    /// If writable
    fn writable(&self) -> bool;
    /// Read file to `UserBuffer`
    fn read(&self, buf: UserBuffer) -> usize;
    /// Write `UserBuffer` to file
    fn write(&self, buf: UserBuffer) -> usize;
}

impl DowncastArc for dyn File {
    fn as_any_arc(self: alloc::sync::Arc<Self>) -> alloc::sync::Arc<dyn Any> {
        // need #![feature(trait_upcasting)]
        self
    }
}
