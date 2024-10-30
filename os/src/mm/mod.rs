mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::{PhysPageNum, VirtAddr};
pub use memory_set::{remap_test, MapPermission, MemorySet, KERNEL_SPACE};
pub use page_table::translated_byte_buffer;

pub fn init() {
    heap_allocator::init_heap();
    // must put after init_heap, coz heap_allocator inited in init_heap()
    // frame_allocator's new & init requires heap (also frame_allocator_test())
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}
