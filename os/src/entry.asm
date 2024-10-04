    .section .text.entry    # .text.entry 区别于其他 .text: 确保该段被放置在相比任何其他代码段更低的地址上
    .globl _start           # globl一个全局符号, 因此可以被其他目标文件使用
_start:
    la sp, boot_stack_top
    call rust_main

    .section .bss.stack             # 下面这块栈空间放在.bss段中
    .globl boot_stack_lower_bound   # 定义lower_bound的位置
boot_stack_lower_bound:
    .space 4096*16                  # 预留了一块大小为 4096*16 字节(64KB)作为栈空间
    .globl boot_stack_top           # 定义top的位置 = lower_bound + 4096*16
boot_stack_top: