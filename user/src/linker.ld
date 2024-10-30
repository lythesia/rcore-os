OUTPUT_ARCH(riscv)
ENTRY(_start)

BASE_ADDRESS = 0x10000;

SECTIONS
{
    . = BASE_ADDRESS;
    .text : {
        *(.text.entry)
        *(.text .text.*)
    }
    /*
        在 .text 和 .rodata 中间以及 .rodata 和 .data 中间我们进行了页面对齐，
        因为前后两个逻辑段的访问方式限制是不同的，由于我们只能以页为单位对这个限制进行设置，
        因此就只能将下一个逻辑段对齐到下一个页面开始放置
    */
    . = ALIGN(4k);
    .rodata : {
        *(.rodata .rodata.*)
    }
    . = ALIGN(4k);
    .data : {
        *(.data .data.*)
    }
    /*
        .data 和 .bss 两个逻辑段由于访问限制相同（可读写），它们中间则无需进行页面对齐
    */
    .bss : {
        *(.bss .bss.*)
    }
    /DISCARD/ : {
        *(.eh_frame)
        *(.debug*)
    }
}