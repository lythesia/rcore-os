use core::arch::global_asm;

use tracer::TracerProvider;

global_asm!(include_str!("kernel_symbol.S"));

extern "C" {
    fn symbol_num();
    fn symbol_address();
    fn symbol_index();
    fn symbol_name();
}

pub struct Provider;

impl TracerProvider for Provider {
    fn address2symbol(&self, addr: usize) -> Option<(usize, &'static str)> {
        find_symbol_with_addr(addr)
    }
}

pub fn find_symbol_with_addr(addr: usize) -> Option<(usize, &'static str)> {
    // println!("find_sym_w/addr {:#x}", addr);
    let symbol_num_addr = symbol_num as usize as *const usize;
    // println!("symbol_num_addr {:#x}", symbol_num_addr as usize);
    let symbol_num = unsafe { symbol_num_addr.read_volatile() };
    // println!("num = {}", symbol_num);
    if symbol_num == 0 {
        return None;
    }
    let symbol_addr = symbol_address as usize as *const usize; // 符号地址存储区域
    let addr_data = unsafe { core::slice::from_raw_parts(symbol_addr, symbol_num) };
    // find the symbol with the nearest address
    let mut index = -1isize;
    for i in 0..symbol_num - 1 {
        if addr >= addr_data[i] && addr < addr_data[i + 1] {
            index = i as isize;
            break;
        }
    }
    if addr == addr_data[symbol_num - 1] {
        index = (symbol_num - 1) as isize;
    }
    // println!("nearest addr to index = {}", index);
    if index == -1 {
        return None;
    }
    let index = index as usize;
    let symbol_index = symbol_index as usize as *const usize; // 符号字符串的起始位置
    let index_data = unsafe { core::slice::from_raw_parts(symbol_index, symbol_num) };
    let symbol_name = symbol_name as usize as *const u8; // 符号字符串
    let mut last = 0;
    unsafe {
        for i in index_data[index].. {
            let c = symbol_name.add(i);
            if *c == 0 {
                last = i;
                break;
            }
        }
    }
    let name = unsafe {
        core::slice::from_raw_parts(symbol_name.add(index_data[index]), last - index_data[index])
    };
    let name = core::str::from_utf8(name).unwrap();
    // println!("symbol = {}", name);
    Some((addr_data[index], name))
}
