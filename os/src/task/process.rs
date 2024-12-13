use alloc::collections::btree_map::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use easy_fs::Inode;

use crate::cast::DowncastArc;
use crate::config::{MMAP_AREA_BASE, PAGE_SIZE};
use crate::fs::{File, OSInode, Stdin, Stdout, ROOT_INODE};
use crate::mm::{
    frame_alloc, translated_refmut, FrameTracker, MapPermission, MemorySet, PageTable, PhysPageNum,
    VPNRange, VirtAddr, VirtPageNum, KERNEL_SPACE,
};
use crate::sync::{Condvar, Mutex, Semaphore, UPIntrFreeCell, UPIntrRefMut};
use crate::trap::{trap_handler, TrapContext};

use super::id::RecycleAllocator;
use super::id::{pid_alloc, PidHandle};
use super::manager::insert_into_pid2process;
use super::task::TaskControlBlock;
use super::{add_task, MMapType, SignalFlags};

/// PCB
pub struct ProcessControlBlock {
    // immutable
    pub pid: PidHandle,
    // mutable
    inner: UPIntrFreeCell<ProcessControlBlockInner>, // use `UPSafeCell` to provide `&self` only to external
}

pub struct ProcessControlBlockInner {
    pub is_zombie: bool,
    pub memory_set: MemorySet,
    pub parent: Option<Weak<ProcessControlBlock>>,
    pub children: Vec<Arc<ProcessControlBlock>>,
    pub exit_code: i32,
    pub fd_table: Vec<Option<Arc<dyn File>>>,
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
    pub signals: SignalFlags,
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    pub task_res_allocator: RecycleAllocator,

    // mmap
    pub mmap_mapped: Vec<MMapReserve>,
    pub mmap_va_allocator: VirtAddressAllocator,
    pub file_mappings: Vec<FileMapping>,

    // cwd
    pub cwd: Arc<Inode>,

    // time stats
    #[allow(unused)]
    pub user_time: usize,
    #[allow(unused)]
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

impl ProcessControlBlockInner {
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }

    pub fn is_zombie(&self) -> bool {
        self.is_zombie
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

    pub fn alloc_tid(&mut self) -> usize {
        self.task_res_allocator.alloc()
    }

    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid);
    }

    pub fn thread_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        self.tasks[tid].as_ref().unwrap().clone()
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

    pub fn tasks_for_each(&self, f: impl Fn(&Arc<TaskControlBlock>)) {
        for t in self.tasks.iter().flatten() {
            f(t)
        }
    }
}

impl ProcessControlBlock {
    pub fn new(elf_data: &[u8]) -> Arc<Self> {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        // alloc pid & kernel stack in kernel space
        let pid_handle = pid_alloc();
        let process = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPIntrFreeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
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
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    // mmap
                    mmap_mapped: Vec::new(),
                    mmap_va_allocator: VirtAddressAllocator::new(MMAP_AREA_BASE),
                    file_mappings: Vec::new(),
                    // cwd
                    cwd: ROOT_INODE.clone(),
                    // time
                    user_time: 0,
                    kernel_time: 0,
                })
            },
        });
        // create main thread
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&process),
            ustack_base,
            true,
        ));
        // prepare trap_cx of main thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        let ustack_top = task_inner.res.as_ref().unwrap().ustack_top();
        let kstack_top = task.kstack.get_top();
        drop(task_inner);
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            ustack_top,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );
        // add main thread to process
        let mut process_inner = process.inner_exclusive_access();
        process_inner.tasks.push(Some(task.clone()));
        drop(process_inner);
        insert_into_pid2process(process.getpid(), process.clone());
        // schedule main thread
        add_task(task);
        process
    }

    pub fn inner_exclusive_access(&self) -> UPIntrRefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }

    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    pub fn fork(self: &Arc<ProcessControlBlock>) -> Arc<ProcessControlBlock> {
        let mut parent_inner = self.inner_exclusive_access();
        assert_eq!(parent_inner.thread_count(), 1);
        // copy parent's user space: including trampoline/ustack's/trap_cx's
        let mut memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        // alloc pid
        let pid_handle = pid_alloc();
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
        let child = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPIntrFreeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    mutex_list: Vec::new(),     // not inherit mutex
                    semaphore_list: Vec::new(), // not inherit sem
                    condvar_list: Vec::new(),   // not inherit cv
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    // mmap
                    mmap_mapped: parent_inner.mmap_mapped.clone(),
                    mmap_va_allocator: parent_inner.mmap_va_allocator.clone(),
                    file_mappings,
                    // cwd
                    cwd: parent_inner.cwd.clone(),
                    // time
                    user_time: 0,
                    kernel_time: 0,
                })
            },
        });
        // add to parent
        parent_inner.children.push(child.clone());
        // create main thread of child
        let task = Arc::new(TaskControlBlock::new(
            child.clone(),
            parent_inner
                .get_task(0)
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .ustack_base(),
            // here we do not allocate trap_cx or ustack again
            // but mention that we allocate a new kstack here
            false,
        ));
        // attach thread to child
        let mut child_inner = child.inner_exclusive_access();
        child_inner.tasks.push(Some(task.clone()));
        drop(child_inner);
        // modify kernel_top in trap_cx
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        trap_cx.kernel_sp = task.kstack.get_top();
        drop(task_inner);
        insert_into_pid2process(child.getpid(), child.clone());
        // schedule child's main thread
        add_task(task);
        child
    }

    pub fn exec(&self, elf_data: &[u8], args: Vec<String>) {
        assert_eq!(self.inner_exclusive_access().thread_count(), 1);
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        let new_token = memory_set.token();
        // substitutes
        self.inner_exclusive_access().memory_set = memory_set;
        let task = self.inner_exclusive_access().get_task(0);
        let mut task_inner = task.inner_exclusive_access();
        // modify ustack
        task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
        // allow user res, after we can get ppn from it
        task_inner.res.as_ref().unwrap().alloc_user_res();
        // get ppn from res, and set back to task
        task_inner.trap_cx_ppn = task_inner.res.as_ref().unwrap().trap_cx_ppn();
        let mut user_sp = task_inner.res.as_ref().unwrap().ustack_top();

        // push arguments on user stack
        // +1 is last 0, indicate end of args
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        // argv is array of ptr, each ptr points to actual str arg
        let mut argv: Vec<_> = (0..=args.len())
            .map(|i| {
                translated_refmut(
                    new_token,
                    (argv_base + i * core::mem::size_of::<usize>()) as *mut usize,
                )
            })
            .collect();
        *argv[args.len()] = 0;
        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            *argv[i] = user_sp;
            let mut p = user_sp;
            for c in args[i].as_bytes() {
                *translated_refmut(new_token, p as *mut u8) = *c;
                p += 1;
            }
            *translated_refmut(new_token, p as *mut u8) = 0; // each str arg end with nul
        }
        // make the user_sp aligned to 8B for k210 platform, but qemu works w/o it
        user_sp -= user_sp % core::mem::size_of::<usize>();

        // init trap_cx
        let mut trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            task.kstack.get_top(),
            trap_handler as usize,
        );
        trap_cx.x[10] = args.len(); // a0 = argc
        trap_cx.x[11] = argv_base; // a1 = argv
        *task_inner.get_trap_cx() = trap_cx;
    }
}
