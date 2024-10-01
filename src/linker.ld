OUTPUT_ARCH(riscv)
ENTRY(_start) /* 指向entry.asm定义的global `_start` */
BASE_ADDRESS = 0x80200000;

SECTIONS
{
    . = BASE_ADDRESS; /* . 表示当前地址 */
    skernel = .;

    stext = .;
    .text : {
        *(.text.entry) /* 包含内核第一条指令的 .text.entry 段放在最终的 .text 段的最开头 */
        *(.text .text.*)
    }

    . = ALIGN(4K);
    etext = .;
    srodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    . = ALIGN(4K);
    erodata = .;
    sdata = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    . = ALIGN(4K);
    edata = .;
    .bss : {
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }

    . = ALIGN(4K);
    ebss = .;
    ekernel = .;

    /DISCARD/ : {
        *(.eh_frame)
    }
}