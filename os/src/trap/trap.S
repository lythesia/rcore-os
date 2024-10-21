.altmacro

.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm

.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm

    .section .text
    // 必须.globl导出全局符号, 否则link error
    // Q: 为什么Ch2的时候没有报错?
    // __alltraps可以解释, 因为在trap/mod.rs里global_asm!了, __alltraps是个local symbol
    // 但是__restore是在batch.rs?
    .globl __alltraps
    .globl __pre_restore
    // __alltraps 的地址 4 字节对齐，这是 RISC-V 特权级规范的要求
    .align 2
__alltraps:
    // 原型是csrrw rd,csr,rs 可以将 CSR 当前的值读到通用寄存器rd中，然后将通用寄存器rs的值写入该 CSR 。因此这里起到的是交换 sscratch 和 sp 的效果
    // 这一行之前 sp 指向用户栈， sscratch 指向内核栈（原因稍后说明），现在 sp 指向内核栈， sscratch 指向用户栈
    csrrw sp, sscratch, sp
    // now sp->kernel stack, sscratch->user stack
    // allocate a TrapContext on kernel stack
    // 我们准备在内核栈上保存 Trap 上下文，于是预先分配 34*8 字节的栈帧
    addi sp, sp, -34*8
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
    // 然后保存到kernel stack上
    // 为什么不直接sd csr 到 mem? Normal load and store instructions are not directly usable with CSR.
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)
    // read user stack from sscratch and save it on the kernel stack
    // 注意：sscratch 的值是进入 Trap 之前的 sp 的值，指向用户栈
    csrr t2, sscratch
    // 因为此时sscratch是用户栈的sp, 正是我们要保存的sp, 所以 mem_of_x2 <- sscratch
    sd t2, 2*8(sp)
    // set input argument of trap_handler(cx: &mut TrapContext)
    // 让寄存器 a0 指向内核栈的栈指针也就是我们刚刚保存的 Trap 上下文的地址，这是由于我们接下来要调用 trap_handler 进行 Trap 处理，
    // 它的第一个参数 cx 由调用规范要从 a0 中获取。而 Trap 处理函数 trap_handler 需要 Trap 上下文的原因在于：
    // 它需要知道其中某些寄存器的值，比如在系统调用的时候应用程序传过来的 syscall ID 和对应参数。
    // 我们不能直接使用这些寄存器现在的值，因为它们可能已经被修改了，因此要去内核栈上找已经被保存下来的值。
    // 解释一下, 在进入trap_handler函数的逻辑以后, 肯定有寄存器被修改, 因此只能从memory(这里是内核栈)上获取已经保存的Trap上下文
    mv a0, sp
    call trap_handler

__restore:
    // 不再需要 在开头 mv sp, a0 了。因为在 __switch 之后，sp 就已经正确指向了我们需要的 Trap 上下文地址。
    // mv sp, a0
    // 从内核栈顶的 Trap 上下文恢复通用寄存器和 CSR
    // now sp->kernel stack(after allocated), sscratch->user stack
    // 如果是第一次运行__restore(来开始运行第一个应用程序)的时候:
    // 1. app_init_context中set_sp把user_stack的sp -> x[2]
    // restore sstatus/sepc
    // 先恢复 CSR , 这样t0-2(x5-7)在后面才能正确恢复
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    // 2. 在这一步x[2] -> t2
    ld t2, 2*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    // 3. 最后一步t2 -> sscratch, 即第一次运行的时候直到此时sscratch才是指向用户栈
    csrw sscratch, t2
    // restore general-purpuse registers except sp/tp
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr
    // release TrapContext on kernel stack
    // 退栈，回归进入 Trap 之前的内核栈栈顶
    addi sp, sp, 34*8
    // now sp->kernel stack, sscratch->user stack
    // 再次交换 sscratch 和 sp，现在 sp 重新指向用户栈栈顶，sscratch 也依然保存进入 Trap 之前的状态并指向内核栈栈顶。
    csrrw sp, sscratch, sp
    // sret 指令回到 U 特权级继续运行应用程序控制流
    sret

// __pre_restore必须放在__restore后, 这里__pre是执行逻辑上的__pre, 不是位置上的
// 因为它只在task第一次运行时执行, 之后都是走__restore(之后的情况是指call trap_handler以后的ra是__restore)
__pre_restore:
    // 因为sp就是栈顶, 栈顶就是TrapContext, 也是switch_cost需要的参数
    mv a0, sp
    call switch_cost
    // call完需要恢复sp
    mv sp, a0
    // 必须跳转到__restore进行正常流程
    j __restore