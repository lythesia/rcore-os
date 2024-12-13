use alloc::vec::Vec;
use lazy_static::lazy_static;
use virtio_drivers::Hal;

use crate::{
    mm::{
        frame_alloc_more, frame_dealloc, kernel_token, FrameTracker, PageTable, PhysAddr,
        PhysPageNum, StepByOne,
    },
    sync::UPIntrFreeCell,
};

lazy_static! {
    static ref QUEUE_FRAMES: UPIntrFreeCell<Vec<FrameTracker>> =
        unsafe { UPIntrFreeCell::new(Vec::new()) };
}

pub struct VirtioHal;
impl Hal for VirtioHal {
    fn dma_alloc(pages: usize) -> virtio_drivers::PhysAddr {
        let trakcers = frame_alloc_more(pages);
        let ppn_base = trakcers.as_ref().unwrap().last().unwrap().ppn;
        QUEUE_FRAMES
            .exclusive_access()
            .append(&mut trakcers.unwrap());
        let pa: PhysAddr = ppn_base.into();
        pa.0
    }

    fn dma_dealloc(paddr: virtio_drivers::PhysAddr, pages: usize) -> i32 {
        let pa = PhysAddr::from(paddr);
        let mut ppn_base: PhysPageNum = pa.into();
        for _ in 0..pages {
            // make sure consecutive!
            frame_dealloc(ppn_base);
            ppn_base.step();
        }
        0
    }

    fn phys_to_virt(paddr: virtio_drivers::PhysAddr) -> virtio_drivers::VirtAddr {
        paddr
    }

    fn virt_to_phys(vaddr: virtio_drivers::VirtAddr) -> virtio_drivers::PhysAddr {
        PageTable::from_token(kernel_token())
            .translate_va(vaddr.into())
            .unwrap()
            .0
    }
}
