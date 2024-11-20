use super::{SignalFlags, MAX_SIG};

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct SignalAction {
    pub handler: usize,
    pub mask: SignalFlags,
}

impl Default for SignalAction {
    fn default() -> Self {
        Self {
            handler: 0,
            mask: SignalFlags::from_bits_truncate(40), // SIGTRAP | SIGQUIT
        }
    }
}

#[derive(Clone, Default)]
pub struct SignalActions {
    pub table: [SignalAction; MAX_SIG + 1],
}
