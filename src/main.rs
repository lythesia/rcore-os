#![no_main]
#![no_std]

mod lang_item;

use core::arch::global_asm;
global_asm!(include_str!("entry.asm"));
