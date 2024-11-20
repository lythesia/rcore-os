use alloc::sync::{Arc, Weak};

use crate::{sync::UPSafeCell, task::suspend_current_and_run_next};

use super::File;

pub struct Pipe {
    readable: bool,
    writable: bool,
    buffer: Arc<UPSafeCell<PipeRingBuffer>>,
}

const RING_BUFFER_SIZE: usize = 32;

#[derive(Clone, Copy, PartialEq)]
enum RingBufferStatus {
    FULL,
    EMPTY,
    NORMAL,
}

pub struct PipeRingBuffer {
    arr: [u8; RING_BUFFER_SIZE],
    head: usize,
    tail: usize,
    status: RingBufferStatus,
    write_end: Option<Weak<Pipe>>, // to tell if all write ends been closed
}

impl PipeRingBuffer {
    pub fn new() -> Self {
        Self {
            arr: [0; RING_BUFFER_SIZE],
            head: 0,
            tail: 0,
            status: RingBufferStatus::EMPTY,
            write_end: None,
        }
    }

    pub fn set_write_end(&mut self, write_end: &Arc<Pipe>) {
        self.write_end = Some(Arc::downgrade(write_end));
    }

    pub fn read_byte(&mut self) -> u8 {
        self.status = RingBufferStatus::NORMAL;
        let c = self.arr[self.head];
        self.head = (self.head + 1) % RING_BUFFER_SIZE;
        if self.head == self.tail {
            self.status = RingBufferStatus::EMPTY;
        }
        c
    }

    pub fn write_byte(&mut self, byte: u8) {
        self.status = RingBufferStatus::NORMAL;
        self.arr[self.tail] = byte;
        self.tail = (self.tail + 1) % RING_BUFFER_SIZE;
        if self.head == self.tail {
            self.status = RingBufferStatus::FULL;
        }
    }

    pub fn available_read(&self) -> usize {
        if self.status == RingBufferStatus::EMPTY {
            0
        } else {
            // | head .. tail |
            if self.tail > self.head {
                self.tail - self.head
            }
            // | .. .. head | .. tail
            //  ___________/
            // / tail .. .. |
            else {
                self.tail + RING_BUFFER_SIZE - self.head
            }
        }
    }

    pub fn available_write(&self) -> usize {
        if self.status == RingBufferStatus::FULL {
            0
        } else {
            RING_BUFFER_SIZE - self.available_read()
        }
    }

    pub fn all_write_ends_closed(&self) -> bool {
        if let Some(weak) = &self.write_end {
            weak.upgrade().is_none()
        } else {
            panic!("PipeRingBuffer write_end not set!")
        }
    }
}

impl Pipe {
    pub fn read_end_with_buffer(buffer: Arc<UPSafeCell<PipeRingBuffer>>) -> Self {
        Self {
            readable: true,
            writable: false,
            buffer,
        }
    }

    pub fn write_end_with_buffer(buffer: Arc<UPSafeCell<PipeRingBuffer>>) -> Self {
        Self {
            readable: false,
            writable: true,
            buffer,
        }
    }
}

impl File for Pipe {
    fn readable(&self) -> bool {
        self.readable
    }

    fn writable(&self) -> bool {
        self.writable
    }

    fn read(&self, buf: crate::mm::UserBuffer) -> usize {
        assert!(self.readable);
        let want_to_read = buf.len();
        let mut buf_iter = buf.into_iter();
        let mut already_read = 0;
        loop {
            let mut rb = self.buffer.exclusive_access();
            // how many bytes allowed to read during this iteration
            let loop_read = rb.available_read();
            if loop_read == 0 {
                // if no more available && all write end closed, that's all we get
                if rb.all_write_ends_closed() {
                    return already_read;
                }
                // else if write end still alive, we wait for more coming
                // aka, suspend and run other task(maybe the one holds writer)
                // to fill ring_buffer, and before that, we must release it
                // to avoid deadlock (coz task switch will not auto drop it)
                drop(rb);
                suspend_current_and_run_next();
                continue;
            }
            // if loop_read bytes can be read
            for _ in 0..loop_read {
                // read into buf one-by-one
                if let Some(byte_ref) = buf_iter.next() {
                    unsafe {
                        *byte_ref = rb.read_byte();
                    }
                    already_read += 1;
                    // return if reach number we need
                    if already_read == want_to_read {
                        return want_to_read;
                    }
                } else {
                    // I think it's exact same with above return condition ..
                    return already_read;
                }
            }
        }
    }

    fn write(&self, buf: crate::mm::UserBuffer) -> usize {
        assert!(self.writable);
        let want_to_write = buf.len();
        let mut buf_iter = buf.into_iter();
        let mut already_write = 0;
        loop {
            let mut rb = self.buffer.exclusive_access();
            let loop_write = rb.available_write();
            if loop_write == 0 {
                drop(rb);
                suspend_current_and_run_next();
                continue;
            }
            for _ in 0..loop_write {
                if let Some(byte_ref) = buf_iter.next() {
                    rb.write_byte(unsafe { *byte_ref });
                    already_write += 1;
                    if already_write == want_to_write {
                        return want_to_write;
                    }
                } else {
                    return already_write;
                }
            }
        }
    }
}

/// Return (read_end, write_end)
pub fn make_pipe() -> (Arc<Pipe>, Arc<Pipe>) {
    let buffer = Arc::new(unsafe { UPSafeCell::new(PipeRingBuffer::new()) });
    let r = Arc::new(Pipe::read_end_with_buffer(buffer.clone()));
    let w = Arc::new(Pipe::write_end_with_buffer(buffer.clone()));
    buffer.exclusive_access().set_write_end(&w);
    (r, w)
}
