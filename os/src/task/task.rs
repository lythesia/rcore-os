use core::cell::RefMut;

use alloc::collections::btree_map::BTreeMap;
use alloc::vec;
use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use easy_fs::Inode;

use crate::cast::DowncastArc;
use crate::config::{MMAP_AREA_BASE, PAGE_SIZE};
use crate::fs::{OSInode, Stdin, Stdout};
use crate::mm::{frame_alloc, FrameTracker, MapPermission, PageTable, VPNRange, VirtPageNum};
use crate::{
    config::TRAP_CONTEXT,
    fs::File,
    mm::{MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE},
    sync::UPSafeCell,
    trap::{trap_handler, TrapContext},
};

use super::MMapType;
use super::{
    context::TaskContext,
    pid::{pid_alloc, KernelStack, PidHandle},
};

#[derive(Clone, Copy, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Zombie,
}

/// TCB
pub struct TaskControlBlock {
    // immutable
    pub pid: PidHandle,
    pub kstack: KernelStack,
    // mutable
    inner: UPSafeCell<TaskControlBlockInner>, // use `UPSafeCell` to provide `&self` only to external
}

pub struct TaskControlBlockInner {
    pub task_status: TaskStatus,
    pub task_cx: TaskContext,
    pub memory_set: MemorySet,
    pub trap_cx_ppn: PhysPageNum,
    #[allow(unused)]
    pub base_size: usize,
    pub parent: Option<Weak<TaskControlBlock>>,
    pub children: Vec<Arc<TaskControlBlock>>,
    pub exit_code: i32,
    pub fd_table: Vec<Option<Arc<dyn File>>>,

    // mmap
    pub mmap_mapped: Vec<MMapReserve>,
    pub mmap_va_allocator: VirtAddressAllocator,
    pub file_mappings: Vec<FileMapping>,

    // time stats
    pub user_time: usize,
    pub kernel_time: usize,
}

#[derive(Clone)]
pub struct MMapReserve {
    pub range: VPNRange,
    pub perm: MapPermission,
    pub ty: MMapType,
}

#[derive(Clone)]
pub struct VirtAddressAllocator {
    curr_va: VirtAddr,
}

impl VirtAddressAllocator {
    pub fn new(base: usize) -> Self {
        Self {
            curr_va: base.into(),
        }
    }

    pub fn alloc(&mut self, len: usize) -> VirtAddr {
        let start = self.curr_va;
        let end: VirtAddr = (start.0 + len).into();
        self.curr_va = end.ceil().into(); // at least 1 page, so allocated `start` is always 4k aligned
        start
    }
}

#[derive(Clone)]
pub struct MapRange {
    /// va_start
    start: VirtAddr,
    /// va_end (exclude)
    end: VirtAddr,
    /// how many bytes mapped
    len: usize,
    /// offset in file, so this range <--> `file[offset..offset+len]`
    offset: usize,
}

impl MapRange {
    pub fn new(start: usize, len: usize, offset: usize) -> Self {
        Self {
            start: start.into(),
            end: (start + len).into(),
            len,
            offset,
        }
    }

    pub fn contains_va(&self, va: &VirtAddr) -> bool {
        &self.start <= va && va < &self.end
    }

    pub fn contains_range(&self, rng: &VPNRange) -> bool {
        self.start.floor() <= rng.get_start() && rng.get_end() <= self.end.ceil()
    }

    pub fn equals_range(&self, rng: &VPNRange) -> bool {
        self.start.floor() == rng.get_start() && self.end.ceil() == rng.get_end()
    }

    pub fn file_offset(&self, vpn: VirtPageNum) -> usize {
        let va: VirtAddr = vpn.into();
        assert!(self.start <= va && va < self.end);
        self.offset + (va.0 - self.start.0)
    }
}

pub struct FileMapping {
    /// which file is mapped, use `inode` instead of `fd:uisze`:
    /// 1. fd which is open can be closed at any time, but mapping holds;
    /// 2. mmap stdin/stdout is meaningless
    file: Arc<Inode>,
    /// same file can be open multiple times, we merge those mappings
    pub ranges: Vec<MapRange>,
    /// manages allocated phys frames
    frames: Vec<FrameTracker>,
    /// file_offset -> ppn
    map: BTreeMap<usize, (VirtPageNum, PhysPageNum)>,
    /// only used for translate vpn, to find pte and check dirty bit
    // TODO: we need slim version, for example, `frames` field is not needed
    pt: PageTable,
}

impl FileMapping {
    pub fn new_empty(file: Arc<Inode>, token: usize) -> Self {
        Self {
            file,
            ranges: Vec::new(),
            frames: Vec::new(),
            map: BTreeMap::new(),
            pt: PageTable::from_token(token),
        }
    }

    /// if exist range contains `va`
    pub fn contains_va(&self, va: &VirtAddr) -> bool {
        self.ranges.iter().any(|range| range.contains_va(va))
    }

    /// Create mapping for given virtual address
    pub fn map(&mut self, va: VirtAddr) -> Option<(PhysPageNum, MapRange, bool)> {
        let vpn = va.floor();

        // 1. find correct range
        for range in &self.ranges {
            if !range.contains_va(&va) {
                continue;
            }
            // 2. offset in file this page mapped to
            let offset = range.file_offset(vpn);
            let (ppn, is_shared) = match self.map.get(&offset) {
                // 3.1 ppn already, 该情况发生在一个进程多次调用mmap映射一个文件, 且file[offset..offset+len]部分有重叠
                // 如:进程A调用mmap -> 访问某个映射的va -> page_fault触发map -> self.map分配实际物理frame
                // 进程A再次调用mmap -> 访问va(这段虚地址和上面不重叠, 但是对应文件的同一个位置) -> page_fault触发map -> self.map发现已经分配过了
                Some(&(_, ppn)) => (ppn, true),
                // 3.2 allocate new ppn
                _ => {
                    let frame = frame_alloc().unwrap();
                    let ppn = frame.ppn;
                    self.frames.push(frame); // frames managed by FileMapping, not MemorySet
                    self.map.insert(offset, (vpn, ppn));
                    (ppn, false)
                }
            };
            return Some((ppn, range.clone(), is_shared));
        }

        None
    }

    /// Write back all dirty pages
    pub fn sync(&self) {
        let file_size = self.file.get_size();
        for (&offset, &(vpn, ppn)) in &self.map {
            // find dirty page
            let pte = self.pt.translate(vpn).unwrap();
            if !pte.is_dirty() {
                continue;
            }
            if offset >= file_size {
                continue;
            }
            let va_len = self
                .ranges
                .iter()
                .map(|r| {
                    if r.offset <= offset && offset < r.offset + r.len {
                        PAGE_SIZE.min(r.offset + r.len - offset)
                    } else {
                        0
                    }
                })
                .max()
                .unwrap();
            let write_len = va_len.min(file_size - offset);
            self.file
                .write_at(offset, &ppn.get_bytes_array()[..write_len]);
        }
    }

    fn copy_to_user(&self, memory_set: &mut MemorySet) -> Self {
        let mut map = BTreeMap::new();
        let mut frames = Vec::new();

        for (&offset, &(vpn, orig_ppn)) in &self.map {
            let frame = frame_alloc().unwrap();
            let ppn = frame.ppn;
            frames.push(frame);
            // map vpn
            let orig_pte = self.pt.translate(vpn).unwrap();
            let map_perm = MapPermission::from_bits_truncate(orig_pte.flags().bits());
            memory_set.map(vpn, ppn, map_perm);
            // copy data
            ppn.get_bytes_array()
                .copy_from_slice(orig_ppn.get_bytes_array());
            // track in map
            map.insert(offset, (vpn, ppn));
        }

        Self {
            file: self.file.clone(),
            ranges: self.ranges.clone(),
            map,
            pt: PageTable::from_token(memory_set.token()),
            frames,
        }
    }

    pub fn file(&self) -> &Arc<Inode> {
        &self.file
    }
}

impl TaskControlBlockInner {
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }

    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }

    fn get_status(&self) -> TaskStatus {
        self.task_status
    }

    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }

    pub fn alloc_fd(&mut self) -> usize {
        match self.fd_table.iter().position(Option::is_none) {
            Some(fd) => fd,
            _ => {
                self.fd_table.push(None);
                self.fd_table.len() - 1
            }
        }
    }

    pub fn find_file_mapping(&mut self, file: &Arc<Inode>) -> Option<&mut FileMapping> {
        let inode_id = file.inode_id();
        self.file_mappings
            .iter_mut()
            .find(|v| v.file.inode_id() == inode_id)
    }

    pub fn copy_file_mappings(&self, new_memory_set: &mut MemorySet) -> Vec<FileMapping> {
        self.file_mappings
            .iter()
            .map(|v| v.copy_to_user(new_memory_set))
            .collect()
    }

    /// if `vpn_range` is free to (map), currently 2 places to check:
    /// 1. `mmap_mapped_ranges` any overlapping range
    /// 2. hard-coded mappings (like `from_elf, from_existed_user`)
    pub fn vpn_range_free(&self, vpn_range: VPNRange) -> bool {
        // 1. cmp with mmap marked ranges
        for v in &self.mmap_mapped {
            if vpn_range.overlap_with(&v.range) {
                return false;
            }
        }

        // 2. cmp with already hard-coded regions (like `from_elf`)
        for vpn in vpn_range {
            match self.memory_set.translate(vpn) {
                Some(pte) if pte.is_valid() => return false,
                _ => {}
            }
        }

        true
    }
}

impl TaskControlBlock {
    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        // alloc pid & kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kstack = KernelStack::new(&pid_handle);
        let kstack_top = kstack.get_top();

        let task_control_block = Self {
            pid: pid_handle,
            kstack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    task_status: TaskStatus::Ready,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    memory_set,
                    trap_cx_ppn,
                    base_size: user_sp,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    mmap_mapped: Vec::new(),
                    mmap_va_allocator: VirtAddressAllocator::new(MMAP_AREA_BASE),
                    file_mappings: Vec::new(),
                    user_time: 0,
                    kernel_time: 0,
                })
            },
        };
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        // println!("task {} borrowed", self.pid.0);
        self.inner.exclusive_access()
    }

    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    pub fn fork(self: &Arc<TaskControlBlock>) -> Arc<TaskControlBlock> {
        let mut parent_inner = self.inner_exclusive_access();
        // copy user space(include trap context)
        let mut memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        // alloc pid & kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kstack = KernelStack::new(&pid_handle);
        let kstack_top = kstack.get_top();
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File>>> = Vec::new();
        for fd in parent_inner.fd_table.iter() {
            if let Some(file) = fd {
                let file_clone = file.clone();
                match file_clone.downcast_arc::<OSInode>() {
                    Some(arc_os_inode) => {
                        let os_inode = arc_os_inode.copy(); // copy file as new os_inode
                        new_fd_table.push(Some(Arc::new(os_inode)));
                    }
                    _ => new_fd_table.push(Some(file.clone())), // currently stdin,stdout,stderr
                }
            } else {
                new_fd_table.push(None);
            }
        }
        // copy file mapping
        let file_mappings = parent_inner.copy_file_mappings(&mut memory_set);
        // construct TCB
        let task_control_block = Arc::new(Self {
            pid: pid_handle,
            kstack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    task_status: TaskStatus::Ready,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    memory_set,
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    fd_table: new_fd_table,
                    exit_code: 0,
                    mmap_mapped: parent_inner.mmap_mapped.clone(),
                    mmap_va_allocator: parent_inner.mmap_va_allocator.clone(),
                    file_mappings,
                    user_time: 0,
                    kernel_time: 0,
                })
            },
        });
        // add to parent
        parent_inner.children.push(task_control_block.clone());

        // modify kernel_sp in trap_cx
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kstack_top;

        task_control_block
    }

    pub fn exec(&self, elf_data: &[u8]) {
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();

        // access inner exclusively
        let mut inner = self.inner_exclusive_access();
        // substitutes
        inner.memory_set = memory_set; // 原有的地址空间会被回收(包括物理frame)
        inner.trap_cx_ppn = trap_cx_ppn;
        // init trap_cx
        let trap_cx = inner.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kstack.get_top(),
            trap_handler as usize,
        );
    }
}
