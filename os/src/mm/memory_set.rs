use core::arch::asm;

use alloc::{collections::btree_map::BTreeMap, sync::Arc, vec::Vec};
use bitflags::bitflags;
use lazy_static::lazy_static;
use riscv::register::satp;

use crate::{
    config::{MEMORY_END, MMIO, PAGE_SIZE, TRAMPOLINE},
    mm::address::StepByOne,
    sync::UPIntrFreeCell,
};

use super::{
    address::{PhysAddr, PhysPageNum, VPNRange, VirtAddr, VirtPageNum},
    frame_allocator::{frame_alloc, FrameTracker},
    page_table::{PTEFlags, PageTable, PageTableEntry},
};

lazy_static! {
    pub static ref KERNEL_SPACE: Arc<UPIntrFreeCell<MemorySet>> =
        Arc::new(unsafe { UPIntrFreeCell::new(MemorySet::new_kernel()) });
}

pub fn kernel_token() -> usize {
    KERNEL_SPACE.exclusive_access().token()
}

extern "C" {
    fn stext();
    fn etext();
    fn srodata();
    fn erodata();
    fn sdata();
    fn edata();
    fn sbss_with_stack();
    fn ebss();
    fn ekernel();
    fn strampoline();
}

/// 一段连续地址的虚拟内存
pub struct MapArea {
    vpn_range: VPNRange,
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
}

impl MapArea {
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        // 必须保证页号所包含的范围 >= [start_va, end_va]
        let s = start_va.floor();
        let e = end_va.ceil();
        Self {
            vpn_range: VPNRange::new(s, e),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }

    // how we `fork` user space
    pub fn from_another(another: &MapArea) -> Self {
        Self {
            vpn_range: another.vpn_range.clone(),
            data_frames: BTreeMap::new(),
            map_type: another.map_type,
            map_perm: another.map_perm,
        }
    }

    pub fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn = match self.map_type {
            MapType::Identical => PhysPageNum(vpn.0),
            MapType::Framed => {
                let frame = frame_alloc().unwrap();
                let p = frame.ppn;
                self.data_frames.insert(vpn, frame);
                p
            }
            MapType::Linear(pn_offset) => {
                // check for sv39
                assert!(vpn.0 < (1usize << 27));
                PhysPageNum((vpn.0 as isize + pn_offset) as usize)
            }
        };
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits).unwrap();
        page_table.map(vpn, ppn, pte_flags);
    }

    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        page_table.unmap(vpn);
    }

    pub fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn);
        }
    }

    #[allow(unused)]
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }

    /// data: start-aligned but maybe with shorter length
    pub fn copy_data(&mut self, _page_table: &PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);

        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();

        // each iteration copy one page
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            // let dst = &mut page_table
            //     .translate(current_vpn)
            //     .unwrap()
            //     .ppn()
            //     .get_bytes_array()[..src.len()];
            let dst = &mut self
                .data_frames
                .get(&current_vpn)
                .unwrap()
                .ppn
                .get_bytes_array()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            current_vpn.step();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapType {
    Identical,
    Framed,
    #[allow(unused)]
    Linear(isize), // offset of page num
}

bitflags! {
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}

pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
}

impl MemorySet {
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
        }
    }

    pub fn new_kernel() -> Self {
        let mut memory_set = Self::new_bare();

        // map trampoline
        memory_set.map_trampoline();

        // map kernel sections
        // println!("mapping kernel .text section");
        memory_set.push(
            MapArea::new(
                (stext as usize).into(),
                (etext as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::X,
            ),
            None,
        );

        // println!("mapping kernel .rodata section");
        memory_set.push(
            MapArea::new(
                (srodata as usize).into(),
                (erodata as usize).into(),
                MapType::Identical,
                MapPermission::R,
            ),
            None,
        );

        // println!("mapping kernel .data section");
        memory_set.push(
            MapArea::new(
                (sdata as usize).into(),
                (edata as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );

        // println!("mapping kernel .bss section");
        memory_set.push(
            MapArea::new(
                (sbss_with_stack as usize).into(),
                (ebss as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );

        // println!("mapping kernel physical memory");
        memory_set.push(
            MapArea::new(
                (ekernel as usize).into(),
                MEMORY_END.into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );

        // println!("mapping memory-mapped registers");
        for &(start, size) in MMIO {
            memory_set.push(
                MapArea::new(
                    start.into(),
                    (start + size).into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::W,
                ),
                None,
            );
        }

        memory_set
    }

    /// Include sections in elf and trampoline and TrapContext and user stack,
    /// also returns user_sp and entry point.
    pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        let mut memory_set = Self::new_bare();

        // map trampoline
        memory_set.map_trampoline();

        // map program headers of elf, with U flag
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");

        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPageNum(0);
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                // log::info!("elf hdr({}): [{:?}, {:?})", ph_flags, start_va, end_va);
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                // Q: 为什么这里不取max而是直接赋值?
                // PT_LOAD Specifies a loadable segment, described by p_filesz and p_memsz.
                // The bytes from the file are mapped to the beginning of the memory segment.
                // If the segment's memory size (p_memsz) is larger than the file size (p_filesz),
                // the extra bytes are defined to hold the value 0 and to follow the segment's
                // initialized area. The file size can not be larger than the memory size.
                // Loadable segment entries in the program header table appear in *ascending* order,
                // sorted on the p_vaddr member.
                // tl;dr segment是按地址升序排放的
                max_end_vpn = map_area.vpn_range.get_end();
                memory_set.push(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        }
        // map user stack with U flags
        let max_end_va: VirtAddr = max_end_vpn.into();
        let mut user_stack_bottom: usize = max_end_va.into();
        // guard page
        user_stack_bottom += PAGE_SIZE;

        (
            memory_set,
            user_stack_bottom,
            elf.header.pt2.entry_point() as usize,
        )
    }

    /// how we `fork` user space
    pub fn from_existed_user(user_space: &MemorySet) -> Self {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        // copy
        for area in &user_space.areas {
            // map areas
            let new_area = MapArea::from_another(area);
            memory_set.push(new_area, None);
            // copy data
            for vpn in area.vpn_range {
                let src = user_space.translate(vpn).unwrap().ppn();
                let dst = memory_set.translate(vpn).unwrap().ppn();
                dst.get_bytes_array().copy_from_slice(src.get_bytes_array());
            }
        }
        memory_set
    }

    /// Mention that trampoline is not collected by areas.
    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(),
            PhysAddr::from(strampoline as usize).into(),
            PTEFlags::R | PTEFlags::X,
        );
    }

    /// Add a new MapArea into this MemorySet.
    /// Assuming that there are no conflicts in the virtual address
    /// space.
    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        map_area.map(&mut self.page_table);
        if let Some(data) = data {
            map_area.copy_data(&self.page_table, data);
        }
        self.areas.push(map_area);
    }

    pub fn activate(&self) {
        let satp = self.page_table.token();
        unsafe {
            /*
            切换 satp CSR 必须是一个 平滑 的过渡：其含义是指，切换 satp 的指令及其下一条指令这两条相邻的指令的虚拟地址是相邻的
            这条写入 satp 的指令及其下一条指令都在内核内存布局的代码段中，在切换之后是一个恒等映射，而在切换之前是视为物理地址直接取指，
            也可以将其看成一个恒等映射。这完全符合我们的期待：即使切换了地址空间，指令仍应该能够被连续的执行。
            */
            satp::write(satp);
            // 清空TLB
            asm!("sfence.vma");
        }
    }

    /// Delegate to page_table
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }

    /// Assume that no conflicts.
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_perm: MapPermission,
    ) {
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, map_perm),
            None,
        );
    }

    /// Delegate `token()` to page_table
    pub fn token(&self) -> usize {
        self.page_table.token()
    }

    pub fn remove_area_with_start_vpn(&mut self, start_vpn: VirtPageNum) {
        if let Some((idx, area)) = self
            .areas
            .iter_mut()
            .enumerate()
            .find(|(_, area)| area.vpn_range.get_start() == start_vpn)
        {
            area.unmap(&mut self.page_table);
            self.areas.remove(idx);
        }
    }

    pub fn recycle_data_pages(&mut self) {
        self.areas.clear();
    }

    /// Delegate `map()` to page_table
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, map_perm: MapPermission) {
        let pte_flags = PTEFlags::from_bits_truncate(map_perm.bits);
        self.page_table.map(vpn, ppn, pte_flags);
    }

    /// Delegate `unmap()` to page_table
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        self.page_table.unmap(vpn);
    }
}

#[allow(unused)]
pub fn remap_test() {
    let mut kernel_space = KERNEL_SPACE.exclusive_access();
    let mid_text: VirtAddr = ((stext as usize + etext as usize) / 2).into();
    let mid_rodata: VirtAddr = ((srodata as usize + erodata as usize) / 2).into();
    let mid_data: VirtAddr = ((sdata as usize + edata as usize) / 2).into();

    assert_eq!(
        kernel_space
            .page_table
            .translate(mid_text.floor())
            .unwrap()
            .writable(),
        false
    );
    assert_eq!(
        kernel_space
            .page_table
            .translate(mid_rodata.floor())
            .unwrap()
            .writable(),
        false
    );
    assert_eq!(
        kernel_space
            .page_table
            .translate(mid_data.floor())
            .unwrap()
            .executable(),
        false
    );
}
