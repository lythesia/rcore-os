#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::{
    get_time, println, sleep,
    syscall::{SYSCALL_EXIT, SYSCALL_GET_TIME, SYSCALL_TASK_INFO, SYSCALL_WRITE, SYSCALL_YIELD},
    task_info, TaskInfo, TaskStatus,
};

#[no_mangle]
pub fn main() -> i32 {
    let t1 = get_time() as usize;
    // info.time is us
    let info = TaskInfo::new();
    get_time();
    sleep(500);
    let t2 = get_time() as usize;
    // 注意本次 task info 调用也计入
    assert_eq!(0, task_info(&info));
    // 注意, 一次println!可能会调用多次sys_write, 可以跟着Write trait的write_fmt去rust-src看, 有如下snip
    // Every spec has a corresponding argument that is preceded by
    // a string piece.
    // for (i, arg) in fmt.iter().enumerate() {
    //     // SAFETY: fmt and args.pieces come from the same Arguments,
    //     // which guarantees the indexes are always within bounds.
    //     let piece = unsafe { args.pieces.get_unchecked(i) };
    //     if !piece.is_empty() {
    //         formatter.buf.write_str(*piece)?;
    //     }
    //     // SAFETY: arg and args.args come from the same Arguments,
    //     // which guarantees the indexes are always within bounds.
    //     unsafe { run(&mut formatter, arg, args.args) }?;
    //     idx += 1;
    // }
    // println!(
    //     "1st task_info: get_time={}, task_info={}, write={}, yield={}, exit={}",
    //     info.syscall_times[SYSCALL_GET_TIME],
    //     info.syscall_times[SYSCALL_TASK_INFO],
    //     info.syscall_times[SYSCALL_WRITE],
    //     info.syscall_times[SYSCALL_YIELD],
    //     info.syscall_times[SYSCALL_EXIT],
    // );
    let t3 = get_time() as usize;
    assert!(3 <= info.syscall_times[SYSCALL_GET_TIME]); // 我认为这里至少4次, 显式调用2次, sleep里2次
    assert_eq!(1, info.syscall_times[SYSCALL_TASK_INFO]);
    assert_eq!(0, info.syscall_times[SYSCALL_WRITE]);
    assert!(0 < info.syscall_times[SYSCALL_YIELD]); // sleep里有yield
    assert_eq!(0, info.syscall_times[SYSCALL_EXIT]);
    let time = info.time / 1000; // us -> ms
    assert!(t2 - t1 <= time + 1);
    assert!(time < t3 - t1 + 100);
    assert!(info.status == TaskStatus::Running);

    println!("string from task info test\n");
    let t4 = get_time() as usize;
    assert_eq!(0, task_info(&info));
    // println!(
    //     "2nd task_info: get_time={}, task_info={}, write={}, yield={}, exit={}",
    //     info.syscall_times[SYSCALL_GET_TIME],
    //     info.syscall_times[SYSCALL_TASK_INFO],
    //     info.syscall_times[SYSCALL_WRITE],
    //     info.syscall_times[SYSCALL_YIELD],
    //     info.syscall_times[SYSCALL_EXIT],
    // );
    let t5 = get_time() as usize * 1000;
    assert!(5 <= info.syscall_times[SYSCALL_GET_TIME]);
    assert_eq!(2, info.syscall_times[SYSCALL_TASK_INFO]);
    assert_eq!(1, info.syscall_times[SYSCALL_WRITE]);
    assert!(0 < info.syscall_times[SYSCALL_YIELD]);
    assert_eq!(0, info.syscall_times[SYSCALL_EXIT]);
    let time = info.time / 1000; // us -> ms
    assert!(t4 - t1 <= time + 1);
    assert!(time < t5 - t1 + 100);
    assert!(info.status == TaskStatus::Running);

    println!("Test task info OK!");
    0
}
