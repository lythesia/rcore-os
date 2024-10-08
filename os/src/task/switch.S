.altmacro

// s_i -> a0[8*(i+2)]
.macro SAVE_SN n
    sd s\n, (\n + 2)*8(a0)
.endm

// s_i <- a0[8*(i+2)]
.macro LOAD_SN n
    ld s\n, (\n + 2)*8(a0)
.endm

.section .text
    .globl __switch
__switch:
    // 阶段 [1]
    // __switch(
    //     current_task_cx_ptr: *mut TaskContext,
    //     next_task_cx_ptr: *const TaskContext
    // )
    // 阶段 [2]
    // save kernel stack of current task
    // sp -> a0[8]
    sd sp, 8(a0)
    // ra -> a0[0]
    sd ra, 0(a0)
    .set n, 0
    .rept 12
        SAVE_SN %n
        .set n, n + 1
    .endr
    // 阶段 [3]
    // restore ra & s0~s11 of next execution
    ld ra, 0(a1)
    .set n, 0
    .rept 12
        LOAD_SN %n
        .set n, n + 1
    .endr
    // restore kernel stack of next task
    ld sp, 8(a1)
    // 阶段 [4]
    ret