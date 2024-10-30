extern "C" {
    fn _num_app();
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
