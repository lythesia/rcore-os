use core::arch::{asm, global_asm};

pub use context::TrapContext;
use riscv::register::{
    scause::{self, Exception, Interrupt},
    sie, sscratch, sstatus, stval, stvec,
    utvec::TrapMode,
};

use crate::{config::TRAMPOLINE, syscall::syscall, task::SignalFlags};

mod context;

global_asm!(include_str!("trap.S"));

pub fn init() {
    set_kernel_trap_entry();
}

fn set_kernel_trap_entry() {
    extern "C" {
        fn __alltraps();
        fn __alltraps_k();
    }
    let __alltraps_k_va = __alltraps_k as usize - __alltraps as usize + TRAMPOLINE;
    unsafe {
        stvec::write(__alltraps_k_va, TrapMode::Direct);
        sscratch::write(trap_from_kernel as usize);
    }
}

fn set_user_trap_entry() {
    unsafe {
        stvec::write(TRAMPOLINE as usize, TrapMode::Direct);
    }
}

/// timer interrupt enabled
pub fn enable_timer_interrupt() {
    unsafe {
        sie::set_stimer(); // 实际是设置了sie寄存器的stie位, 使得S特权级时钟中断不会被屏蔽
    }
}

fn enable_supervisor_interrupt() {
    unsafe {
        sstatus::set_sie();
    }
}

fn disable_supervisor_interrupt() {
    unsafe {
        sstatus::clear_sie();
    }
}

#[no_mangle]
/// handle an interrupt, exception, or system call from user space
pub fn trap_handler() -> ! {
    set_kernel_trap_entry();
    crate::task::user_time_end();
    let scause = scause::read();
    let stval = stval::read();

    match scause.cause() {
        scause::Trap::Exception(Exception::UserEnvCall) => {
            // 应用的 Trap 上下文不在内核地址空间，因此我们调用 current_trap_cx 来获取当前应用的 Trap 上下文的可变引用
            // 而不是像之前那样作为参数传入 trap_handler
            let cx = crate::task::current_trap_cx();
            cx.sepc += 4;
            enable_supervisor_interrupt();
            let ret = syscall(cx.x[17], [cx.x[10], cx.x[11], cx.x[12]]);
            // cx is changed during sys_exec, so we have to call it again
            let cx = crate::task::current_trap_cx();
            cx.x[10] = ret as usize;
        }
        scause::Trap::Exception(Exception::StoreFault)
        | scause::Trap::Exception(Exception::LoadFault)
        | scause::Trap::Exception(Exception::InstructionFault)
        | scause::Trap::Exception(Exception::InstructionPageFault)
        | scause::Trap::Exception(Exception::StorePageFault)
        | scause::Trap::Exception(Exception::LoadPageFault) => {
            if !crate::task::handle_page_fault(stval) {
                // log::error!("[kernel] {:?} in application, bad addr = {:#x}, bad instruction = {:#x}, core dumped.",
                //     scause.cause(),
                //     stval,
                //     crate::task::current_trap_cx().sepc
                // );
                crate::task::current_add_signal(SignalFlags::SIGSEGV);
            }
        }
        scause::Trap::Exception(Exception::IllegalInstruction) => {
            // let p = crate::task::current_process();
            // log::error!("[kernel] {:?} in application, bad addr = {:#x}, bad instruction = {:#x}, core dumped. pid = {}",
            //     scause.cause(),
            //     stval,
            //     crate::task::current_trap_cx().sepc,
            //     p.getpid(),
            // );
            crate::task::current_add_signal(SignalFlags::SIGILL);
        }
        scause::Trap::Interrupt(Interrupt::SupervisorTimer) => {
            crate::timer::set_next_trigger();
            crate::timer::check_timer();
            crate::task::suspend_current_and_run_next();
        }
        scause::Trap::Interrupt(Interrupt::SupervisorExternal) => {
            crate::board::irq_handler();
        }
        _ => {
            panic!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
    // handle signals
    crate::task::current_handle_signals();
    // check error signals (if error then exit)
    if let Some((errno, msg)) = crate::task::check_signals_error_of_current() {
        println!("[kernel] {}", msg);
        crate::task::exit_current_and_run_next(errno);
    }

    crate::task::user_time_start();
    trap_return()
}

#[no_mangle]
pub fn trap_return() -> ! {
    disable_supervisor_interrupt();
    // stvec指向TRAMPOLINE, 我们已经map过从它开始的一个page(@MemorySet::map_trampoline), 指向物理地址的trap.S的code
    // 其实就是__alltraps, 这样之后app在trap的时候会跳转到__alltraps
    set_user_trap_entry();
    // __restore的参数a0, Trap 上下文在应用地址空间中的虚拟地址
    // MUST use thread scope trap_cx addr, NOT the fixed TRAP_CONTEXT one!
    let trap_cx_ptr = crate::task::current_trap_cx_user_va();
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
pub fn trap_from_kernel(_trap_cx: &TrapContext) {
    let scause = scause::read();
    let stval = stval::read();
    match scause.cause() {
        scause::Trap::Interrupt(Interrupt::SupervisorExternal) => {
            crate::board::irq_handler();
        }
        scause::Trap::Interrupt(Interrupt::SupervisorTimer) => {
            crate::timer::set_next_trigger();
            crate::timer::check_timer();
            // do not schedule now
        }
        _ => {
            panic!(
                "Unsupported trap from kernel: {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
}
