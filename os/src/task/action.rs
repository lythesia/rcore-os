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

impl SignalActions {
    pub fn is_masked(&self, signum: usize, signal: SignalFlags) -> bool {
        assert!(signum <= MAX_SIG);
        self.table[signum].mask.contains(signal)
    }

    pub fn get_handler(&self, signum: usize) -> usize {
        assert!(signum <= MAX_SIG);
        self.table[signum].handler
    }

    pub fn get_action(&self, signum: usize) -> SignalAction {
        assert!(signum <= MAX_SIG);
        self.table[signum]
    }

    pub fn set_action(&mut self, signum: usize, action: SignalAction) {
        assert!(signum <= MAX_SIG);
        self.table[signum] = action;
    }
}
