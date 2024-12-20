/// Task Context
#[repr(C)]
pub struct TaskContext {
    /// ra
    ra: usize,
    /// sp
    sp: usize,
    /// s0 - s11
    s: [usize; 12],
}

impl TaskContext {
    pub fn zero_init() -> Self {
        TaskContext {
            ra: 0,
            sp: 0,
            s: [0; 12],
        }
    }

    pub fn goto_trap_return(kstack_ptr: usize) -> Self {
        Self {
            ra: crate::trap::trap_return as usize,
            sp: kstack_ptr,
            s: [0; 12],
        }
    }
}
