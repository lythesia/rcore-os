use crate::{
    mm::{MapPermission, VirtAddr},
    task,
};

const VA_MAX: usize = usize::MAX;

/// `start` must be 4k aligned, `prot` arrange as (hi->lo) xwr
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    // 4k align
    if start & 0xfff != 0 ||
    // only last 3 bits allowed
        prot & !0x7 != 0 ||
    // no perm specified, meaningless
        prot & 0x7 == 0 ||
    // theoretically no space (avoid overflow)
        VA_MAX - len <= start
    {
        return -1;
    }

    let start_vpn = VirtAddr::from(start).floor();
    let end_vpn = VirtAddr::from(start + len).ceil();
    let map_perm = MapPermission::from_bits_truncate((prot << 1) as u8) | MapPermission::U;
    task::current_task_map_new_area(start_vpn, end_vpn, map_perm)
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    // 4k align
    if start & 0xfff != 0 ||
    // theoretically no space
    VA_MAX - len <= start
    {
        return -1;
    }

    let start_vpn = VirtAddr::from(start).floor();
    let end_vpn = VirtAddr::from(start + len).ceil();
    task::current_task_unmap_area(start_vpn, end_vpn)
}
