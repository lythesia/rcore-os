use core::any::Any;

use alloc::sync::Arc;

pub trait DowncastArc: Any {
    /// must be implemented: cast `Arc<Self>` into `Arc<dyn Any>`
    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any>;

    /// cast `Arc<dyn impl Trait>` into `Arc<T>`
    fn downcast_arc<T: Any>(self: Arc<Self>) -> Option<Arc<T>> {
        let arc_any: Arc<dyn Any> = self.as_any_arc();
        if arc_any.is::<T>() {
            // will not change ref-count
            let ptr = Arc::into_raw(arc_any);
            Some(unsafe { Arc::from_raw(ptr as *const T) })
        } else {
            None
        }
    }
}
