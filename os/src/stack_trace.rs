use core::arch::asm;

pub unsafe fn print_stack_trace() {
    println!("== Begin stack trace ==");

    let mut fp: *const usize;
    // asm ref: https://doc.rust-lang.org/nightly/rust-by-example/unsafe/asm.html
    asm!("mv {}, fp", out(reg) fp);

    while !fp.is_null() {
        let saved_fp = *fp.sub(2);
        println!("fp = 0x{:016x}", saved_fp);
        fp = saved_fp as *const usize;
    }
    println!("== End stack trace ==");
}
