use core::fmt::Debug;

use crate::config::{PAGE_SIZE, PAGE_SIZE_BITS};

use super::page_table::PageTableEntry;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub usize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(pub usize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNum(pub usize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtPageNum(pub usize);

/// debugging

impl Debug for PhysAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("PA:{:#x}", self.0))
    }
}
impl Debug for VirtAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("VA:{:#x}", self.0))
    }
}
impl Debug for PhysPageNum {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("PPN:{:#x}", self.0))
    }
}
impl Debug for VirtPageNum {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("VPN:{:#x}", self.0))
    }
}

/// T: {PhysAddr, VirtAddr, PhysPageNum, VirtPageNum}
/// T -> usize: T.0
/// usize -> T: usize.into()

const PA_WIDTH_SV39: usize = 56;
const VA_WIDTH_SV39: usize = 39;
const PPN_WIDTH_SV39: usize = PA_WIDTH_SV39 - PAGE_SIZE_BITS; // 44
const VPN_WIDTH_SV39: usize = VA_WIDTH_SV39 - PAGE_SIZE_BITS; // 27

const PA_MASK: usize = (1 << PA_WIDTH_SV39) - 1;
const VA_MASK: usize = (1 << VA_WIDTH_SV39) - 1;
pub(super) const PPN_MASK: usize = (1 << PPN_WIDTH_SV39) - 1;
pub(super) const VPN_MASK: usize = (1 << VPN_WIDTH_SV39) - 1;
const PG_OFS_MASK: usize = PAGE_SIZE - 1;

// usize <-> PhysAddr
impl From<usize> for PhysAddr {
    fn from(value: usize) -> Self {
        Self(value & PA_MASK)
    }
}
impl From<PhysAddr> for usize {
    fn from(value: PhysAddr) -> Self {
        value.0
    }
}

impl PhysAddr {
    pub fn page_offset(&self) -> usize {
        self.0 & PG_OFS_MASK
    }

    pub fn floor(&self) -> PhysPageNum {
        PhysPageNum(self.0 / PAGE_SIZE)
    }

    pub fn ceil(&self) -> PhysPageNum {
        PhysPageNum((self.0 + PAGE_SIZE - 1) / PAGE_SIZE)
    }

    pub fn get_ref<T>(&self) -> &'static T {
        unsafe { (self.0 as *const T).as_ref().unwrap() }
    }

    pub fn get_mut<T>(&self) -> &'static mut T {
        unsafe { (self.0 as *mut T).as_mut().unwrap() }
    }
}

// usize <-> PhysPageNum
impl From<usize> for PhysPageNum {
    fn from(value: usize) -> Self {
        Self(value & PPN_MASK)
    }
}
impl From<PhysPageNum> for usize {
    fn from(value: PhysPageNum) -> Self {
        value.0
    }
}

// 对物理空间的三种粒度的访问方式
impl PhysPageNum {
    /// 基于bytes的物理空间访问(limit为一个page, 4K)
    // Q: why & -> &'static?
    // we don't care if `pa` dropped, coz `pa.0`(which is usize only) is cast to pointer
    // and we just need the byte array by that pointer
    pub fn get_bytes_array(&self) -> &'static mut [u8] {
        let pa: PhysAddr = (*self).into();
        unsafe { core::slice::from_raw_parts_mut(pa.0 as *mut u8, 4096) }
    }

    /// 基于PTE的物理空间访问(cast为512个PTE的数组, 总大小也是4K)
    pub fn get_pte_array(&self) -> &'static mut [PageTableEntry] {
        let pa: PhysAddr = (*self).into();
        unsafe { core::slice::from_raw_parts_mut(pa.0 as *mut PageTableEntry, 512) }
    }

    /// 基于任意类型T的物理空间访问(cast为T类型)
    pub fn get_mut<T>(&self) -> &'static mut T {
        let pa: PhysAddr = (*self).into();
        unsafe { (pa.0 as *mut T).as_mut().unwrap() }
    }
}

// PhysPageNum <-> PhysAddr
impl From<PhysPageNum> for PhysAddr {
    fn from(value: PhysPageNum) -> Self {
        Self(value.0 << PAGE_SIZE_BITS)
    }
}
impl From<PhysAddr> for PhysPageNum {
    fn from(value: PhysAddr) -> Self {
        assert_eq!(value.page_offset(), 0);
        value.floor()
    }
}

// usize <-> VirtAddr
impl From<usize> for VirtAddr {
    fn from(value: usize) -> Self {
        Self(value & VA_MASK)
    }
}
impl From<VirtAddr> for usize {
    fn from(value: VirtAddr) -> Self {
        value.0
    }
}

impl VirtAddr {
    pub fn page_offset(&self) -> usize {
        self.0 & PG_OFS_MASK
    }

    pub fn floor(&self) -> VirtPageNum {
        VirtPageNum(self.0 / PAGE_SIZE)
    }

    pub fn ceil(&self) -> VirtPageNum {
        VirtPageNum((self.0 + PAGE_SIZE - 1) / PAGE_SIZE)
    }

    pub fn aligned(&self) -> bool {
        self.page_offset() == 0
    }
}

// usize <-> VirtPageNum
impl From<usize> for VirtPageNum {
    fn from(value: usize) -> Self {
        Self(value & VPN_MASK)
    }
}
impl From<VirtPageNum> for usize {
    fn from(value: VirtPageNum) -> Self {
        value.0
    }
}

impl VirtPageNum {
    /*
    |26    18|17     9|8      0|
    +--------+--------+--------+
    | VPN[0] | VPN[1] | VPN[2] |
    +--------+--------+--------+
    */
    pub fn indexes(&self) -> [usize; 3] {
        let mut vpn = self.0;
        let mut idx = [0; 3];
        for i in (0..3).rev() {
            idx[i] = vpn & 0x1ff;
            vpn >>= 9;
        }
        idx
    }
}

// VirtPageNum <-> VirtAddr
impl From<VirtPageNum> for VirtAddr {
    fn from(value: VirtPageNum) -> Self {
        Self(value.0 << PAGE_SIZE_BITS)
    }
}
impl From<VirtAddr> for VirtPageNum {
    fn from(value: VirtAddr) -> Self {
        assert_eq!(value.page_offset(), 0);
        value.floor()
    }
}

pub trait StepByOne {
    fn step(&mut self);
}
impl StepByOne for PhysPageNum {
    fn step(&mut self) {
        self.0 += 1;
    }
}
impl StepByOne for VirtPageNum {
    fn step(&mut self) {
        self.0 += 1;
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct SimpleRange<T>
where
    T: StepByOne + Copy + Debug + PartialEq + PartialOrd,
{
    l: T,
    r: T,
}
impl<T> Debug for SimpleRange<T>
where
    T: StepByOne + Copy + Debug + PartialEq + PartialOrd,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[{:?}, {:?})", self.l, self.r)
    }
}

impl<T> SimpleRange<T>
where
    T: StepByOne + Copy + Debug + PartialEq + PartialOrd,
{
    pub fn new(s: T, e: T) -> Self {
        assert!(s <= e, "start {:?} > end {:?}!", s, e);
        Self { l: s, r: e }
    }

    pub fn get_start(&self) -> T {
        self.l
    }

    pub fn get_end(&self) -> T {
        self.r
    }

    pub fn contains(&self, v: impl Into<T>) -> bool {
        let vpn: T = v.into();
        self.l <= vpn && vpn < self.r // r excluded
    }

    pub fn overlap_with(&self, other: &Self) -> bool {
        // coz rhs is excluded, we must make lhs+1 when comparing
        let mut other_l = other.get_start();
        other_l.step();
        let mut self_l = self.l.clone();
        self_l.step();

        self.r >= other_l && other.get_end() >= self_l
    }
}

impl<T> IntoIterator for SimpleRange<T>
where
    T: StepByOne + Copy + Debug + PartialEq + PartialOrd,
{
    type Item = T;

    type IntoIter = SimpleRangeIterator<T>;

    fn into_iter(self) -> Self::IntoIter {
        SimpleRangeIterator::new(self.l, self.r)
    }
}

pub struct SimpleRangeIterator<T>
where
    T: StepByOne + Copy + Debug + PartialEq + PartialOrd,
{
    current: T,
    end: T,
}

impl<T> SimpleRangeIterator<T>
where
    T: StepByOne + Copy + PartialEq + PartialOrd + Debug,
{
    pub fn new(l: T, r: T) -> Self {
        Self { current: l, end: r }
    }
}

impl<T> Iterator for SimpleRangeIterator<T>
where
    T: StepByOne + Copy + PartialEq + PartialOrd + Debug,
{
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.end {
            None
        } else {
            let t = self.current;
            self.current.step();
            Some(t)
        }
    }
}

pub type VPNRange = SimpleRange<VirtPageNum>;
