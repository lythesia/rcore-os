use std::{fs::File, io::Write, path::Path};

const TARGET_PATH: &str = "../user/target/riscv64gc-unknown-none-elf/release/";

fn main() {
    println!("cargo:rerun-if-changed=../user/src");
    println!("cargo:rerun-if-changed={}", TARGET_PATH);
    println!("cargo:rerun-if-changed={}", "src");
    let path = Path::new("src/trace/kernel_symbol.S");
    if !path.exists() {
        let mut f = File::create(path).unwrap();
        writeln!(
            f,
            r#"
    .section .rodata
    .align 3
    .globl symbol_num
    .globl symbol_address
    .globl symbol_index
    .globl symbol_name
symbol_num:
    .quad {}
symbol_address:
symbol_index:
symbol_name:"#,
            0
        )
        .unwrap();
    }
}
