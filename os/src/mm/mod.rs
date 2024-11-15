mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::*;
pub use frame_allocator::{frame_alloc, frame_dealloc, FrameTracker};
pub use memory_set::{kernel_token, remap_test, MapPermission, MemorySet, KERNEL_SPACE};
pub use page_table::*;

pub fn init() {
    heap_allocator::init_heap();
    // must put after init_heap, coz heap_allocator inited in init_heap()
    // frame_allocator's new & init requires heap (also frame_allocator_test())
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}
