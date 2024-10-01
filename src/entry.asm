    .section .text.entry    # .text.entry 区别于其他 .text: 确保该段被放置在相比任何其他代码段更低的地址上
    .globl _start           # 一个全局符号，因此可以被其他目标文件使用
_start:
    li x1, 100              # x1 <- 100