use core::cell::OnceCell;

use alloc::format;
use bitflags::bitflags;

use crate::{
    fs::{File, OSInode},
    mm::{MapPermission, VPNRange, VirtAddr},
    task::{self, FileMapping, MMapReserve, MMapType, MapRange},
};

const VA_MAX: usize = usize::MAX;

bitflags! {
    pub struct MMapFlags: u32 {
        const MAP_ANON = 0; // not mapping file, but memory area
        const MAP_FILE = 1 << 0; // mapping file, but no memory area
        const MAP_FIXED = 1 << 1; // map to fixed addr, can be used together with either of above
    }
}

/// `start` must be 4k aligned (if used with MAP_FIXED), if zero let kernel decide mapped base address;
///
/// `prot` arrange as (hi->lo) xwr;
///
/// `flags` MAP_ANON, MAP_FILE(using `fd` and `offset`), MAP_FIXED(using `start`);
///
/// `fd` should `>2`;
///
/// `offset` ref to `start`;
pub fn sys_mmap(
    start: usize,
    len: usize,
    prot: usize,
    flags: usize,
    fd: usize,
    offset: usize,
) -> isize {
    // flags
    let mmap_flags = MMapFlags::from_bits_truncate(flags as u32);
    // MAP_FIXED used, need to check start
    if mmap_flags.contains(MMapFlags::MAP_FIXED) {
        // start 4k align
        // invalid len: 0
        // theoretically no space (avoid overflow)
        if start & 0xfff != 0 || len == 0 || start >= VA_MAX - len {
            return -1;
        }
    }

    // prot
    // only last 3 bits allowed
    // no perm specified
    if prot & !0x7 != 0 || prot & 0x7 == 0 {
        return -1;
    }

    let map_perm = MapPermission::from_bits_truncate((prot << 1) as u8) | MapPermission::U;
    if mmap_flags.contains(MMapFlags::MAP_FILE) {
        do_mmap_file(start, len, map_perm, mmap_flags, fd, offset)
    } else {
        do_mmap_memory(start, len, map_perm, mmap_flags)
    }
}

fn do_mmap_file(
    start: usize,
    len: usize,
    map_perm: MapPermission,
    mmap_flags: MMapFlags,
    fd: usize,
    offset: usize,
) -> isize {
    use crate::cast::DowncastArc;

    if fd <= 2 {
        return -1;
    }

    let proc = task::current_process();
    let mut inner = proc.inner_exclusive_access();

    // if fd open
    let fp = match inner.fd_table.get(fd) {
        Some(Some(v)) => v.clone(),
        _ => return -1,
    };
    let inode = match fp.downcast_arc::<OSInode>() {
        Some(v) if v.is_file() => v, // must be regular file
        _ => return -1,
    };
    // check fd perm consistancy with map_perm
    if map_perm.contains(MapPermission::R) && !inode.readable()
        || map_perm.contains(MapPermission::W) && !inode.writable()
    {
        return -1;
    }
    // check file size
    let file = inode.clone_inner_inode();
    let file_size = file.get_size();
    drop(inode);
    if len > file_size || offset > file_size - len {
        return -1;
    }

    // mapped vpns: [start_vpn, end_vpn]
    let start_va = if mmap_flags.contains(MMapFlags::MAP_FIXED) {
        VirtAddr::from(start)
    } else {
        inner.mmap_va_allocator.alloc(len)
    };
    let start_vpn = start_va.floor();
    let end_vpn = VirtAddr::from(start_va.0 + len).ceil();
    let vpn_range = VPNRange::new(start_vpn, end_vpn);
    // check availability
    if !inner.vpn_range_free(vpn_range) {
        return -1;
    }

    // lazy mapping
    inner.mmap_mapped.push(MMapReserve {
        range: vpn_range,
        perm: map_perm,
        ty: MMapType::File,
    });
    match inner.find_file_mapping(&file) {
        Some(m) => m.ranges.push(MapRange::new(start_va.0, len, offset)),
        _ => {
            let mut m = FileMapping::new_empty(file, inner.memory_set.token());
            m.ranges.push(MapRange::new(start_va.0, len, offset));
            inner.file_mappings.push(m);
        }
    }

    let start_va: VirtAddr = start_vpn.into();
    start_va.0 as isize
}

fn do_mmap_memory(
    start: usize,
    len: usize,
    map_perm: MapPermission,
    mmap_flags: MMapFlags,
) -> isize {
    let proc = task::current_process();
    let mut inner = proc.inner_exclusive_access();

    let start_va = if mmap_flags.contains(MMapFlags::MAP_FIXED) {
        VirtAddr::from(start)
    } else {
        inner.mmap_va_allocator.alloc(len)
    };
    let start_vpn = start_va.floor();
    let end_vpn = VirtAddr::from(start_va.0 + len).ceil();
    let vpn_range = VPNRange::new(start_vpn, end_vpn);
    // check availability
    if !inner.vpn_range_free(vpn_range) {
        return -1;
    }

    // lazy mapping
    inner.mmap_mapped.push(MMapReserve {
        range: vpn_range,
        perm: map_perm,
        ty: MMapType::Memory,
    });

    let start_va: VirtAddr = start_vpn.into();
    start_va.0 as isize
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    // 4k align
    if start & 0xfff != 0 ||
    // theoretically no space
    VA_MAX - len <= start
    {
        return -1;
    }

    let proc = task::current_process();
    let mut inner = proc.inner_exclusive_access();
    let start_vpn = VirtAddr::from(start).floor();
    let end_vpn = VirtAddr::from(start + len).ceil();
    let vpn_range = VPNRange::new(start_vpn, end_vpn);

    // 1. find in mmap_mapped
    // NOT allow partially unmap!
    let idx = match inner.mmap_mapped.iter().position(|v| v.range == vpn_range) {
        Some(v) => v,
        _ => return -1, // not mapped before
    };
    // we `get` but not `remove` here, coz unmap process may fail, we keep atomic
    let MMapReserve { ty, .. } = inner.mmap_mapped[idx].clone();

    match ty {
        // 2.1 unmap if mem
        MMapType::Memory => {
            for vpn in vpn_range {
                match inner.memory_set.translate(vpn) {
                    Some(pte) if !pte.is_valid() => {
                        return -1; // not valid, what happend?!
                    }
                    Some(_) => inner.memory_set.unmap(vpn),
                    _ => {
                        // no entry due to lazy alloc, which means this vpn has never been used
                    }
                }
            }
        }
        // 2.2 complex if file
        MMapType::File => {
            // we can only find ONE range in ONE file_mapping here
            // every `range` of `file_mapping` stands for ONE mmap call
            // TODO can we optimize this?
            let mut i = OnceCell::new();
            let mut j = OnceCell::new();
            for (_i, f) in inner.file_mappings.iter().enumerate() {
                for (_j, r) in f.ranges.iter().enumerate() {
                    if r.contains_range(&vpn_range) {
                        j.set(_j)
                            .expect(&format!("multiple FileMapping found for {:?}", vpn_range));
                        i.set(_i)
                            .expect(&format!("multiple MapRange found for {:?}", vpn_range));
                    }
                }
            }
            // see if we need recycle file_mapping.ranges, or even file_mapping itself
            let i = i.take().unwrap();
            let j = j.take().unwrap();
            // always do sync before recycle attempts
            inner.file_mappings[i].sync();
            // try recycle range
            if inner.file_mappings[i].ranges[j].equals_range(&vpn_range) {
                inner.file_mappings[i].ranges.remove(j);
            }
            // try recyle file_mapping
            if inner.file_mappings[i].ranges.is_empty() {
                inner.file_mappings.remove(i);
            }
            // unmap MUST after sync, coz sync uses transate to find pte, we need to check pte flags
            for vpn in vpn_range {
                inner.memory_set.unmap(vpn);
            }
        }
    }

    // 3. remove from mmap_mapped
    inner.mmap_mapped.remove(idx);

    0
}
