use core::cell::{RefCell, RefMut};

pub struct UPSafeCell<T> {
    inner: RefCell<T>,
}

unsafe impl<T> Sync for UPSafeCell<T> {}

impl<T> UPSafeCell<T> {
    pub unsafe fn new(v: T) -> Self {
        Self {
            inner: RefCell::new(v),
        }
    }

    pub fn exclusive_access(&self) -> RefMut<'_, T> {
        // Panics if the value is currently borrowed
        self.inner.borrow_mut()
    }
}
