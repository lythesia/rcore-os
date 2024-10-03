#![no_main]
#![no_std]

mod config;
mod console;
mod lang_item;
mod logging;
mod sbi;
mod timer;

use core::arch::global_asm;
global_asm!(include_str!("entry.asm"));

// 使用fn不用static usize的好处是后续使用不用unsafe block:
// use of extern static is unsafe and requires unsafe function or block
// extern statics are not controlled by the Rust type system: invalid data, aliasing violations or data races will cause undefined behavior
extern "C" {
    fn stext();
    fn etext();

    fn srodata();
    fn erodata();

    fn sdata();
    fn edata();

    fn sbss();
    fn ebss();
}

#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    logging::init();

    log::debug!("hello, {} @ {}", "rust_main", "os");

    log::info!(".text [{:#x}, {:#x})", stext as usize, etext as usize);
    log::info!(".rodata [{:#x}, {:#x})", srodata as usize, erodata as usize);
    log::info!(".data [{:#x}, {:#x})", sdata as usize, edata as usize);
    log::info!(".bss [{:#x}, {:#x})", sbss as usize, ebss as usize);

    log::warn!("sleep 5s ..");
    timer::sleep_us(5_000_000);
    log::warn!("wake!");
    log::error!("die!");

    panic!("Shutdown machine!");
}

fn clear_bss() {
    // 尝试从其他地方找到全局符号 sbss 和 ebss, 它们在linker.ld中定义

    // Q: 请问为什么Rust通过外部调用访问linker中的全局符号是通过函数的形式？
    // 在Rust的手册中对于extern的使用方法只简要介绍了C函数调用，不知道到哪里找相应的规范。
    // 查找到在C代码中调用链接脚本中的变量的方式是将其作为extern int A导入，其地址&A是对应的符号地址。
    // 在这里Rust使用的两个函数sbss()及ebss()返回的就是相应全局符号的地址吗？而之后还是通过变量的方式进行使用，看起来其实还是某种调用规范？

    // A: 在extern "C"里面提到的sbss和ebss就只是两个在其他位置（链接脚本linker-qemu.ld中）声明的全局符号，
    // 我们期望在链接的时候这两个符号能正确被修改为它们所在的地址，进而才能知道.bss段的位置并完成初始化。那我们怎么做呢？
    // 目前只能想到用FFI的方式来引入，根据官方文档，在extern "C"块中似乎只能引用ABI接口，也就是一个函数签名，需要有函数名、参数列表和返回值。
    // 好像不能像C语言那样extern int c;这样做。引入之后sbss和ebss都变成函数了，所以有as usize将其转换成函数入口地址也就是符号自身的地址。

    // Cond: 不对，其实是能类似于`extern int c;`这样用的, 例子
    // https://github.com/rustsbi/rustsbi-qemu/blob/main/rustsbi-qemu/src/main.rs#L70
    // extern "C" {
    //     fn sbss();
    //     fn ebss();
    // }
    // 起始和终止地址, 遍历该地址区间并逐字节进行清零即可
    // (sbss as usize..ebss as usize).for_each(|a| unsafe {
    //     (a as *mut u8).write_volatile(0);
    // });
    unsafe {
        (sbss as usize..ebss as usize).for_each(|a| (a as *mut u8).write_volatile(0));
    }
}
