use core::arch::{asm, global_asm};

pub use context::TrapContext;
use riscv::register::{
    scause::{self, Exception, Interrupt},
    sie, stval, stvec,
};

use crate::{
    config::{TRAMPOLINE, TRAP_CONTEXT},
    syscall::syscall,
};

mod context;

global_asm!(include_str!("trap.S"));

pub fn init() {
    set_kernel_trap_entry();
}

fn set_kernel_trap_entry() {
    unsafe {
        stvec::write(trap_from_kernel as usize, stvec::TrapMode::Direct);
    }
}

fn set_user_trap_entry() {
    unsafe {
        stvec::write(TRAMPOLINE as usize, stvec::TrapMode::Direct);
    }
}

/// timer interrupt enabled
pub fn enable_timer_interrupt() {
    unsafe {
        sie::set_stimer(); // 实际是设置了sie寄存器的stie位, 使得S特权级时钟中断不会被屏蔽
    }
}

#[no_mangle]
/// handle an interrupt, exception, or system call from user space
pub fn trap_handler() -> ! {
    crate::task::user_time_end();
    // 应用的 Trap 上下文不在内核地址空间，因此我们调用 current_trap_cx 来获取当前应用的 Trap 上下文的可变引用
    // 而不是像之前那样作为参数传入 trap_handler
    let cx = crate::task::current_trap_cx();
    let scause = scause::read();
    let stval = stval::read();

    match scause.cause() {
        scause::Trap::Exception(Exception::UserEnvCall) => {
            cx.sepc += 4;
            let syscall_id = cx.x[17];
            cx.x[10] = syscall(syscall_id, [cx.x[10], cx.x[11], cx.x[12]]) as usize;
        }
        scause::Trap::Exception(Exception::StorePageFault)
        | scause::Trap::Exception(Exception::LoadPageFault) => {
            log::error!("[kernel] PageFault in application, kernel killed it.");
            crate::task::exit_current_and_run_next();
        }
        scause::Trap::Exception(Exception::IllegalInstruction) => {
            log::error!("[kernel] IllegalInstruction in application, kernel killed it.");
            crate::task::exit_current_and_run_next();
        }
        scause::Trap::Interrupt(Interrupt::SupervisorTimer) => {
            crate::timer::set_next_trigger();
            crate::task::suspend_current_and_run_next();
        }
        _ => {
            panic!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
    crate::task::user_time_start();
    trap_return()
}

#[no_mangle]
pub fn trap_return() -> ! {
    // stvec指向TRAMPOLINE, 我们已经map过从它开始的一个page(@MemorySet::map_trampoline), 指向物理地址的trap.S的code
    // 其实就是__alltraps, 这样之后app在trap的时候会跳转到__alltraps
    set_user_trap_entry();
    // __restore的参数a0, Trap 上下文在应用地址空间中的虚拟地址
    let trap_cx_ptr = TRAP_CONTEXT;
    // __restore的参数a1, 应用地址空间的 token
    let user_satp = crate::task::current_user_token();

    extern "C" {
        fn __alltraps();
        fn __restore();
    }

    // Q: 如何找到 __restore 在内核/应用地址空间中共同的虚拟地址
    // A: 由于 __alltraps 是对齐到地址空间跳板页面的起始地址 TRAMPOLINE 上的，则 __restore 的虚拟地址
    //    只需在 TRAMPOLINE 基础上加上 __restore 相对于 __alltraps 的偏移量即可
    let restore_va = __restore as usize - __alltraps as usize + TRAMPOLINE;
    // fence.i 指令清空指令缓存 i-cache 。这是因为，在内核中进行的一些操作可能导致一些原先存放某个应用代码的物理页帧
    // 如今用来存放数据或者是其他应用的代码，i-cache 中可能还保存着该物理页帧的错误快照
    unsafe {
        asm!(
            "fence.i",
            "jr {restore_va}",
            restore_va = in(reg) restore_va,
            in("a0") trap_cx_ptr,
            in("a1") user_satp,
            options(noreturn)
        );
    }
}

#[no_mangle]
pub unsafe fn pre_trap_return() -> ! {
    crate::task::SWITCH_TIME_COUNT += crate::timer::get_time_us() - crate::task::SWITCH_TIME_START;
    trap_return()
}

#[no_mangle]
pub fn trap_from_kernel() -> ! {
    panic!("a trap from kernel!");
}
