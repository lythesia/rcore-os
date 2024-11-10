use crate::{mm, task};

const FD_STDIN: usize = 0;
const FD_STDOUT: usize = 1;

/// write buf of length `len` to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    match fd {
        FD_STDOUT => {
            let bufs = mm::translated_byte_buffer(task::current_user_token(), buf, len);
            for buf in bufs {
                let str = core::str::from_utf8(buf).unwrap();
                print!("{}", str);
            }
            len as isize
        }
        _ => {
            panic!("Unsupported fd in sys_write!");
        }
    }
}

/// read buf of length `len` from a file with `fd`
pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    match fd {
        FD_STDIN => {
            assert_eq!(len, 1, "Only support len = 1 in sys_read!");
            let mut c: usize;
            loop {
                c = crate::sbi::console_getchar();
                if c == 0 {
                    task::suspend_current_and_run_next();
                    continue;
                } else {
                    break;
                }
            }
            let ch = c as u8;
            let mut bufs = mm::translated_byte_buffer(task::current_user_token(), buf, len);
            unsafe {
                bufs[0].as_mut_ptr().write_volatile(ch);
            }
            1
        }
        _ => {
            panic!("Unsupported fd in sys_read!");
        }
    }
}
