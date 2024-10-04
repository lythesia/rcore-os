use core::arch::global_asm;

pub use context::TrapContext;
use riscv::register::{
    scause::{self, Exception},
    stval, stvec,
};

use crate::{batch::run_next_app, println, syscall::syscall};

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

#[no_mangle]
/// handle an interrupt, exception, or system call from user space
pub fn trap_handler(cx: &mut TrapContext) -> &mut TrapContext {
    let scause = scause::read();
    let stval = stval::read();

    match scause.cause() {
        scause::Trap::Exception(Exception::UserEnvCall) => {
            // 由 ecall 指令触发的系统调用，在进入 Trap 的时候，硬件会将 sepc 设置为这条 ecall 指令所在的地址
            // 而在 Trap 返回之后，我们希望应用程序控制流从 ecall 的下一条指令开始执行
            // 因此我们只需修改 Trap 上下文里面的 sepc，让它增加 ecall 指令的码长，也即 4 字节
            // 这样在 __restore 的时候 sepc 在恢复之后就会指向 ecall 的下一条指令
            cx.sepc += 4;
            cx.x[10] = syscall(cx.x[17], [cx.x[10], cx.x[11], cx.x[12]]) as usize;
        }
        scause::Trap::Exception(Exception::StoreFault)
        | scause::Trap::Exception(Exception::StorePageFault) => {
            println!("[kernel] PageFault in application, kernel killed it.");
            run_next_app();
        }
        scause::Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, kernel killed it.");
            run_next_app();
        }
        _ => {
            panic!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
    // 传入的Trap 上下文 cx 原样返回 (其实是cx的地址)
    // 联系trap.S的__restore开头: mv sp, a0
    // 返回值在a0, 而a0指向cx即栈顶, sp此时也是栈顶, 所以sp <- a0无影响, 即对应case2
    cx
}
