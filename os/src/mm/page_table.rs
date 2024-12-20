use alloc::vec::Vec;
use alloc::{string::String, vec};
use bitflags::bitflags;

use super::PhysAddr;
use super::{
    address::{PhysPageNum, StepByOne, VirtPageNum, PPN_MASK},
    frame_allocator::{frame_alloc, FrameTracker},
    VirtAddr,
};

bitflags! {
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

/*
PTE

|63     54|53    28|27    19|18    10|9   8| 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
+---------+--------+--------+--------+-----+---+---+---+---+---+-------+---+
|reserved | PPN[2] | PPN[1] | PPN[0] | RSW | D | A | G | U | X | W | R | V |
+---------+--------+--------+--------+-----+---+---+---+---+---+-------+---+
*/
#[derive(Clone, Copy)]
#[repr(C)]
pub struct PageTableEntry {
    pub bits: usize,
}

impl PageTableEntry {
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        Self {
            bits: ppn.0 << 10 | flags.bits as usize,
        }
    }

    pub fn empty() -> Self {
        // V = 0 also, so it's an illegal PTE
        Self { bits: 0 }
    }

    pub fn ppn(&self) -> PhysPageNum {
        const PPN_MASK: usize = (1 << 44) - 1;
        (self.bits >> 10 & PPN_MASK).into()
    }

    pub fn flags(&self) -> PTEFlags {
        const FLAGS_MASK: usize = (1 << 8) - 1;
        PTEFlags::from_bits((self.bits & FLAGS_MASK) as u8).unwrap()
    }

    pub fn is_valid(&self) -> bool {
        self.flags().contains(PTEFlags::V)
    }

    pub fn readable(&self) -> bool {
        self.flags().contains(PTEFlags::R)
    }

    pub fn writable(&self) -> bool {
        self.flags().contains(PTEFlags::W)
    }

    pub fn executable(&self) -> bool {
        self.flags().contains(PTEFlags::X)
    }

    pub fn is_dirty(&self) -> bool {
        self.flags().contains(PTEFlags::D)
    }
}

pub struct PageTable {
    root_ppn: PhysPageNum,
    frames: Vec<FrameTracker>,
}

impl PageTable {
    pub fn new() -> Self {
        let frame = frame_alloc().unwrap();
        Self {
            root_ppn: frame.ppn,
            frames: vec![frame],
        }
    }

    /// Temporarily used to get arguments from user space.
    pub fn from_token(satp: usize) -> Self {
        Self {
            root_ppn: PhysPageNum::from(satp & PPN_MASK),
            frames: Vec::new(),
        }
    }

    pub fn token(&self) -> usize {
        // MODE = 8, enable paging
        8 << 60 | self.root_ppn.0
    }

    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.find_pte(vpn).map(|pte| pte.clone())
    }

    pub fn translate_va(&self, va: VirtAddr) -> Option<PhysAddr> {
        self.find_pte(va.floor()).map(|pte| {
            let aligned_pa: PhysAddr = pte.ppn().into();
            let offset = va.page_offset();
            let pa_usize: usize = aligned_pa.into();
            (pa_usize + offset).into()
        })
    }

    /*
        假设完全从empty开始
        1. 首先root_ppn是new的时候产生的, 且分配了一个frame (root_ppn == base)
        2. i=0: 在root_ppn基址[base, base + 0x1000)的一个frame内, 按vpn[0]的偏移(0 - 511)找到目录项(大小8byte)
            如果invalid, 则分配一个frame0: ppn = root_ppn + 1(frame_base + 1*0x1000)
        3. i=1: 在frame0_ppn, 同上按vpn[1]寻找
            如果invalid, 则分配一个frame1: ppn = root_ppn + 2(frame_base + 2*0x1000)
        4. i=2: 同上
            如果invalid, 则分配一个frame2: ppn = root_ppn + 3(frame_base + 3*0x1000)
        5. 结束, frame2就是对应的pte
    */
    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idx = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result = None;

        for i in 0..3 {
            // 以ppn为基址, 找到第i级, 即vpn[i]的偏移对应的entry
            let pte = &mut ppn.get_pte_array()[idx[i]];
            // 如果是末级, 直接返回
            if i == 2 {
                result = Some(pte);
                break;
            }
            // 当 V 为 0 的时候, 表当前指针是一个空指针, 无法走向下一级节, 则创建一个节点
            if !pte.is_valid() {
                let frame = frame_alloc().unwrap();
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V);
                self.frames.push(frame);
            }
            // 以新节点为基址, 继续寻找
            ppn = pte.ppn();
        }

        result
    }

    fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idx = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result = None;

        for i in 0..3 {
            let pte = &mut ppn.get_pte_array()[idx[i]];
            // 这里在if里面并没有再判断pte是否合法，而是将pte直接包裹起来返回。
            // 所以find_pte可能返回一个不合法（即标志位V为0）的页表项。
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                return None;
            }
            ppn = pte.ppn();
        }

        result
    }

    /// insert kv
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) {
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(!pte.is_valid(), "vpn {:?} is mapped before mapping", vpn);
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V);
    }

    /// remove kv
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        let pte = self.find_pte(vpn).unwrap();
        assert!(pte.is_valid(), "vpn {:?} is invalid before unmapping", vpn);
        *pte = PageTableEntry::empty();
    }
}

/// translate buffer of `[ptr, ptr+len]` in `token` space
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
    let page_table = PageTable::from_token(token);
    let mut start = ptr as usize;
    let end = start + len;
    let mut v = Vec::new();
    // 每个iter获取一段连续的空间, 为什么分段?
    // 因为虚地址连续的一个buffer, 对应的物理地址空间不一定连续, 所以按4K(一个page大小)来获取每一段
    while start < end {
        let start_va = VirtAddr::from(start);
        // 1. 获取start_va开始的vpn(aligned)
        let mut vpn = start_va.floor();
        // 2. 获取对应ppn
        let ppn = page_table.translate(vpn).unwrap().ppn();
        // 3. vpn+1
        vpn.step();
        // 4. 当前连续段的end只能是min { aligned_vpn_addr, end } (其实非end一定是aligned)
        let end_va = VirtAddr::from(vpn).min(VirtAddr::from(end));
        // 5. 非end时, 当前page的[start_va.offset..]都是需要的空间; end时, 则到end_va.offset为止
        if end_va.page_offset() == 0 {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..]);
        } else {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()]);
        }
        start = end_va.into();
    }
    v
}

/// translate str of `ptr`(read until '\0') in `token` space
pub fn translated_str(token: usize, ptr: *const u8) -> String {
    let page_table = PageTable::from_token(token);
    let mut start_va = VirtAddr::from(ptr as usize);
    let mut vpn = start_va.floor();
    let mut s = String::new();
    loop {
        let ppn = page_table.translate(vpn).unwrap().ppn();
        let bytes = &ppn.get_bytes_array()[start_va.page_offset()..];
        let slice = match bytes.split_once(|&c| c == 0) {
            Some((v, _)) => v,
            _ => bytes,
        };
        s.push_str(core::str::from_utf8(slice).unwrap());

        if slice.len() < bytes.len() {
            break;
        }
        vpn.step();
        start_va = vpn.into();
    }
    s
}

pub fn translate_ref<T>(token: usize, ptr: *const T) -> &'static T {
    let page_table = PageTable::from_token(token);
    let va = VirtAddr::from(ptr as usize);
    let pa = page_table.translate_va(va).unwrap();
    pa.get_ref()
}

/// for primitive values only
// Q: https://github.com/rcore-os/rCore-Tutorial-Book-v3/issues/55#issuecomment-1568718900
// A: compiler保证这些值的地址是aligned, 即不会cross page boundary
// see https://github.com/rcore-os/rCore-Tutorial-v3/pull/80
pub fn translated_refmut<T>(token: usize, ptr: *mut T) -> &'static mut T {
    let page_table = PageTable::from_token(token);
    let va = VirtAddr::from(ptr as usize);
    let pa = page_table.translate_va(va).unwrap();
    pa.get_mut()
}

/// Array of u8 slice that user communicate with os
pub struct UserBuffer {
    pub buffers: Vec<&'static mut [u8]>,
}
impl UserBuffer {
    pub fn new(buffers: Vec<&'static mut [u8]>) -> Self {
        Self { buffers }
    }

    pub fn len(&self) -> usize {
        self.buffers.iter().map(|b| b.len()).sum()
    }
}

pub struct UserBufferIterator {
    buffers: Vec<&'static mut [u8]>,
    curr_buf: usize,
    pos_in_buf: usize,
}

impl IntoIterator for UserBuffer {
    type Item = *mut u8;

    type IntoIter = UserBufferIterator;

    fn into_iter(self) -> Self::IntoIter {
        UserBufferIterator {
            buffers: self.buffers,
            curr_buf: 0,
            pos_in_buf: 0,
        }
    }
}

impl Iterator for UserBufferIterator {
    type Item = *mut u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr_buf >= self.buffers.len() {
            return None;
        }

        let r = &mut self.buffers[self.curr_buf][self.pos_in_buf] as *mut _;
        // last in curr_buf
        if self.pos_in_buf + 1 == self.buffers[self.curr_buf].len() {
            self.pos_in_buf = 0;
            self.curr_buf += 1;
        } else {
            self.pos_in_buf += 1;
        }
        Some(r)
    }
}
