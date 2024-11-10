use crate::mm::{MapPermission, VPNRange, VirtPageNum};

use super::processor;

pub fn current_task_map_new_area(
    start_vpn: VirtPageNum,
    end_vpn: VirtPageNum,
    map_perm: MapPermission,
) -> isize {
    let task = processor::current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    let curr_mem_set = &mut inner.memory_set;
    for vpn in VPNRange::new(start_vpn, end_vpn) {
        match curr_mem_set.translate(vpn) {
            Some(pte) if pte.is_valid() => return -1, // mapped already
            _ => {}
        }
    }
    curr_mem_set.insert_framed_area(start_vpn.into(), end_vpn.into(), map_perm);
    0
}

pub fn current_task_unmap_area(start_vpn: VirtPageNum, end_vpn: VirtPageNum) -> isize {
    let task = processor::current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    let curr_mem_set = &mut inner.memory_set;
    for vpn in VPNRange::new(start_vpn, end_vpn) {
        match curr_mem_set.translate(vpn) {
            Some(pte) if !pte.is_valid() => return -1, // not valid
            Some(_) => curr_mem_set.page_table_mut().unmap(vpn),
            _ => return -1, // no entry
        }
    }
    0
}
