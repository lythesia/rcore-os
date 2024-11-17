use alloc::sync::Arc;

use crate::{config::PAGE_SIZE, mm::VirtAddr};

use super::{processor, MMapReserve};

#[derive(Clone, Copy)]
pub enum MMapType {
    Memory,
    File,
}

/// Try to handle page fault caused by demand paging
/// Returns whether this page fault is fixed
pub fn handle_page_fault(fault_addr: usize) -> bool {
    let fault_va: VirtAddr = fault_addr.into();
    let fault_vpn = fault_va.floor();
    let task = processor::current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();

    match task_inner.memory_set.translate(fault_vpn) {
        // already mapped
        // 如果上一次page_fault被处理了, 这里pte就是valid, 所以返回false
        // 换句话说这次page_fault不是缺页, 而是其他异常, 比如读写权限问题
        Some(pte) if pte.is_valid() => return false,
        _ => {}
    }

    let MMapReserve { range, perm, ty } = match task_inner
        .mmap_mapped
        .iter()
        .find(|v| v.range.contains(fault_vpn))
    {
        Some(v) => v.clone(),
        _ => return false,
    };

    match ty {
        MMapType::Memory => {
            let start_va = range.get_start().into();
            let end_va = range.get_end().into();
            task_inner
                .memory_set
                .insert_framed_area(start_va, end_va, perm);
        }
        MMapType::File => {
            // check file_mappings
            let mapping = match task_inner
                .file_mappings
                .iter_mut()
                .find(|v| v.contains_va(&fault_va))
            {
                Some(v) => v,
                _ => return false,
            };
            let file = Arc::clone(mapping.file());

            // phys frame allocated
            let (ppn, range, is_shared) = mapping.map(fault_va).unwrap();
            // setup va-pa mapping
            task_inner.memory_set.map(fault_vpn, ppn, perm);

            // load file
            if !is_shared {
                let file_size = file.get_size();
                let file_offset = range.file_offset(fault_vpn);
                assert!(file_offset < file_size);

                let read_len = PAGE_SIZE.min(file_size - file_offset);
                let buf = &mut ppn.get_bytes_array()[..read_len];
                file.read_at(file_offset, buf);
            }
        }
    }

    true
}
