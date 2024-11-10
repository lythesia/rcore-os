use alloc::vec::Vec;
use lazy_static::lazy_static;

extern "C" {
    fn _num_app();
    fn _app_names();
}

lazy_static! {
    static ref APP_NAMES: Vec<&'static str> = {
        let num_app = get_num_app();
        let mut start = _app_names as usize as *const u8;
        let mut vs = Vec::new();
        unsafe {
            for _ in 0..num_app {
                let mut end = start;
                while end.read_volatile() != '\0' as u8 {
                    end = end.add(1);
                }
                let slice = core::slice::from_raw_parts(start, end as usize - start as usize);
                let str = core::str::from_utf8(slice).unwrap();
                vs.push(str);
                start = end.add(1);
            }
        }
        vs
    };
}

/// Get the total number of applications.
pub fn get_num_app() -> usize {
    unsafe { (_num_app as usize as *const usize).read_volatile() }
}

/// Get app data(in elf) of index i
pub fn get_app_data(i: usize) -> &'static [u8] {
    let ptr = _num_app as usize as *const usize;
    let num_app = get_num_app();

    let app_start = unsafe { core::slice::from_raw_parts(ptr.add(1), num_app + 1) };
    assert!(i < num_app);
    unsafe {
        core::slice::from_raw_parts(app_start[i] as *const u8, app_start[i + 1] - app_start[i])
    }
}

/// Get app data(in elf) by name
pub fn get_app_data_by_name(name: &str) -> Option<&'static [u8]> {
    let num_app = get_num_app();
    (0..num_app)
        .find(|&i| APP_NAMES[i] == name)
        .map(get_app_data)
}

/// List loaded apps
pub fn list_apps() {
    println!("/**** APPS ****");
    for app in APP_NAMES.iter() {
        println!("{}", app);
    }
    println!("**************/");
}
