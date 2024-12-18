.altmacro

.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm

.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm

    .section .text.trampoline
    .globl __alltraps
    .globl __restore
    .globl __alltraps_k
    .globl __restore_k
    // __alltraps 的地址 4 字节对齐，这是 RISC-V 特权级规范的要求
    .align 2
__alltraps:
    csrrw sp, sscratch, sp
    // now sp->*TrapContext in user space, sscratch->user stack
    // save general-purpose registers
    sd x1, 1*8(sp)
    // skip sp(x2), we will save it later
    sd x3, 3*8(sp)
    // skip tp(x4), application does not use it
    // save x5~x31
    .set n, 5
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr
    // we can use t0/t1/t2 freely, because they were saved on kernel stack
    // csrr rd, csr 的功能就是将 CSR 的值读到寄存器rd中, 这里t0 <- sstatus, t1 <- sepc
    // t0-2是x5-7, 上面已经保存了, 所以这里直接使用
    csrr t0, sstatus
    csrr t1, sepc
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)
    // read user stack from sscratch and save it in TrapContext
    csrr t2, sscratch
    sd t2, 2*8(sp)
    // load kernel_satp into t0
    ld t0, 34*8(sp)
    // load trap_handler into t1
    ld t1, 36*8(sp)
    // move to kernel_sp
    ld sp, 35*8(sp)
    // switch to kernel space
    csrw satp, t0
    sfence.vma
    // jump to trap_handler
    // 为什么不能沿用call trap_handler?
    // 本段.text.trampoline和trap_handler在同一个段内, trap_handler的偏移是个固定值, call是伪指令,
    // 会编译成直接跳转这个偏移量; 但实际上trap_handler(我们需要跳转的)的虚地址在最高页内, 并不是这个偏移
    // 得到的地址
    // Q: 我还有问题, 如果trap_handler的代码都是一样的, 跳转到哪走的逻辑不都是一样?
    // A: 注意jr上面的两条指令是切换地址空间, 如果使用call (== pc + offset), 那么切换换地址空间以后,
    // pc + offset还是trap_handler吗? 显然不是了
    jr t1

__restore:
    // a0: *TrapContext in user space(Constant); a1: user space token
    // switch to user space
    csrw satp, a1
    sfence.vma
    csrw sscratch, a0
    mv sp, a0
    // now sp points to TrapContext in user space, start restoring based on it
    // restore sstatus/sepc
    // 先恢复 CSR , 这样t0-2(x5-7)在后面才能正确恢复
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    // restore general-purpuse registers except sp/tp
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr
    // 直接恢复sp, x[2] -> sp, 即回到user stack
    ld sp, 2*8(sp)
    sret

    .align 2
__alltraps_k:
    addi sp, sp, -34*8
    sd x1, 1*8(sp)
    sd x3, 3*8(sp)
    .set n, 5
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr
    csrr t0, sstatus
    csrr t1, sepc
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)
    mv a0, sp
    csrr t2, sscratch
    jalr t2

__restore_k:
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr
    addi sp, sp, 34*8
    sret