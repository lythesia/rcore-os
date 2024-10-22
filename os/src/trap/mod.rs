use core::arch::global_asm;

pub use context::TrapContext;
use riscv::register::{
    scause::{self, Exception, Interrupt},
    sie, stval, stvec,
};

use crate::{syscall::syscall, task, timer};

mod context;

global_asm!(include_str!("trap.S"));

/// initialize CSR `stvec` as the entry of `__alltraps`
pub fn init() {
    extern "C" {
        fn __alltraps();
    }

    unsafe {
        stvec::write(__alltraps as usize, stvec::TrapMode::Direct);
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
pub fn trap_handler(cx: &mut TrapContext) -> &mut TrapContext {
    crate::task::user_time_end();
    let scause = scause::read();
    let stval = stval::read();

    match scause.cause() {
        scause::Trap::Exception(Exception::UserEnvCall) => {
            // 由 ecall 指令触发的系统调用，在进入 Trap 的时候，硬件会将 sepc 设置为这条 ecall 指令所在的地址
            // 而在 Trap 返回之后，我们希望应用程序控制流从 ecall 的下一条指令开始执行
            // 因此我们只需修改 Trap 上下文里面的 sepc，让它增加 ecall 指令的码长，也即 4 字节
            // 这样在 __restore 的时候 sepc 在恢复之后就会指向 ecall 的下一条指令
            cx.sepc += 4;
            let syscall_id = cx.x[17];
            task::current_task_record_syscall(syscall_id);
            cx.x[10] = syscall(syscall_id, [cx.x[10], cx.x[11], cx.x[12]]) as usize;
        }
        scause::Trap::Exception(Exception::StoreFault)
        | scause::Trap::Exception(Exception::StorePageFault) => {
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
    // 传入的Trap 上下文 cx 原样返回 (其实是cx的地址)
    // 联系trap.S的__restore开头: mv sp, a0
    // 返回值在a0, 而a0指向cx即栈顶, sp此时也是栈顶, 所以sp <- a0无影响, 即对应case2
    // 在ch3中, 这个返回值没有用到, 因为sp是kernel_stack栈顶, 已经是我们需要的
    cx
}

#[no_mangle]
pub unsafe fn switch_cost(cx: &mut TrapContext) -> &mut TrapContext {
    task::SWITCH_TIME_COUNT += timer::get_time_us() - task::SWITCH_TIME_START;
    cx
}
