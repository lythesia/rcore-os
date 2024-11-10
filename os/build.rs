use std::fs::{read_dir, File};
use std::io::{Error, Result, Write};

static TARGET_PATH: &str = "../user/build/elf";

fn main() {
    println!("cargo:rerun-if-changed=../user/src");
    println!("cargo:rerun-if-changed={}", TARGET_PATH);
    insert_app_data().unwrap();
}

fn insert_app_data() -> Result<()> {
    let mut f = File::create("src/link_app.S")?;
    let mut apps = read_dir(TARGET_PATH)?
        .into_iter()
        .map(|e| {
            let mut fname = e?
                .file_name()
                .into_string()
                .map_err(|e| Error::other(format!("invalid os_string of file: {e:?}")))?;
            if let Some(dot) = fname.rfind('.') {
                fname.drain(dot..);
            }
            Ok(fname)
        })
        .collect::<Result<Vec<_>>>()?;
    if apps.is_empty() {
        return Ok(());
    }
    apps.sort();

    writeln!(
        f,
        r#"
    .align 3
    .section .data
    .globl _num_app
_num_app:
    .quad {}"#,
        apps.len()
    )?;

    for i in 0..apps.len() {
        writeln!(f, "    .quad app_{}_start", i)?;
    }
    writeln!(f, "    .quad app_{}_end", apps.len() - 1)?;

    writeln!(
        f,
        r#"    .globl _app_names
_app_names:"#,
    )?;
    for app in &apps {
        writeln!(f, r#"    .string "{}""#, app)?;
    }

    for (i, app) in apps.iter().enumerate() {
        println!("+app_{}: {}", i, app);
        writeln!(
            f,
            r#"
    .section .data
    .globl app_{0}_start
    .globl app_{0}_end
    .align 3
app_{0}_start:
    .incbin "{1}/{2}.elf"
app_{0}_end:"#,
            i, TARGET_PATH, app
        )?;
    }
    Ok(())
}
