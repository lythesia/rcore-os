#![feature(assert_matches)]
use easy_fs::{BlockDevice, EasyFileSystem, BLOCK_SZ};
use structopt::StructOpt;

use std::{
    assert_matches::assert_matches,
    fs::{read_dir, File, OpenOptions},
    io::{Error, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};

struct BlockFile(Mutex<File>);

impl BlockDevice for BlockFile {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        let mut file = self.0.lock().unwrap();
        file.seek(SeekFrom::Start((block_id * BLOCK_SZ) as u64))
            .expect("Error seeking!");
        assert_matches!(file.read(buf), Ok(BLOCK_SZ), "Not a complete block!");
    }

    fn write_block(&self, block_id: usize, buf: &[u8]) {
        let mut file = self.0.lock().unwrap();
        file.seek(SeekFrom::Start((block_id * BLOCK_SZ) as u64))
            .expect("Error seeking!");
        assert_matches!(file.write(buf), Ok(BLOCK_SZ), "Not a complete block!");
    }

    fn handle_irq(&self) {
        unimplemented!()
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "EasyFileSystem packe")]
struct Opt {
    #[structopt(short, long, help = "Executable source dir(with backslash)")]
    source: PathBuf,
    #[structopt(
        short,
        long,
        help = "Executable target dir(with backslash)",
        parse(from_os_str)
    )]
    target: PathBuf,
}

fn easy_fs_pack() -> std::io::Result<()> {
    let opt = Opt::from_args();
    println!("easy-fs-fuse: {opt:?}");

    let block_file = Arc::new(BlockFile(Mutex::new({
        let path = opt.target.join("fs.img");
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        f.set_len(32 * 2048 * 512)?;
        f
    })));
    // 32MiB block dev; bitmap 1 block == at most 4095 files
    let efs = EasyFileSystem::create(block_file, 32 * 2048, 1);
    let root_inode = Arc::new(EasyFileSystem::root_inode(&efs));
    let apps = read_dir(opt.source.as_path())?
        .into_iter()
        .map(|dirent| {
            let mut fname = dirent?
                .file_name()
                .into_string()
                .map_err(|e| Error::other(format!("invalid os_string of file: {e:?}")))?;
            if let Some(dot) = fname.rfind('.') {
                fname.drain(dot..); // remove .rs ext
            }
            Ok(fname)
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    println!("easy-fs-use >>>>");
    let mut size_total = 0;
    for app in apps {
        // load built app only from host file system
        let path = opt.target.join(&app);
        // skip un-built
        if !std::fs::exists(&path)? {
            continue;
        }
        let mut host_file = File::open(path)?;
        let mut all_data = Vec::new();
        host_file.read_to_end(&mut all_data)?;
        println!(
            "easy-fs-fuse: + {app} {}B {}KB",
            all_data.len(),
            all_data.len() / 1024
        );
        size_total += all_data.len();
        // create a file in easy-fs
        let inode = root_inode.create(&app).unwrap();
        // write data to easy-fs
        inode.write_at(0, &all_data);
    }
    println!("easy-fs-use (total: {}KB) <<<<", size_total / 1024);
    Ok(())
}

fn main() {
    easy_fs_pack().expect("Error when packing easy-fs!");
}

#[cfg(test)]
mod tests {
    use super::*;
    use easy_fs::Inode;

    #[test]
    fn efs_test() -> std::io::Result<()> {
        let block_file = Arc::new(BlockFile(Mutex::new({
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open("target/fs.img")?;
            f.set_len(8192 * 512).unwrap();
            f
        })));
        EasyFileSystem::create(block_file.clone(), 4096, 1);
        let efs = EasyFileSystem::open(block_file.clone());
        let root_inode = EasyFileSystem::root_inode(&efs);
        root_inode.create("filea");
        root_inode.create("fileb");
        for name in root_inode.ls() {
            println!("{}", name);
        }
        let filea = root_inode.find("filea").unwrap();
        let greet_str = "Hello, world!";
        filea.write_at(0, greet_str.as_bytes());
        //let mut buffer = [0u8; 512];
        let mut buffer = [0u8; 233];
        let len = filea.read_at(0, &mut buffer);
        assert_eq!(greet_str, core::str::from_utf8(&buffer[..len]).unwrap(),);

        let mut random_str_test = |len: usize| {
            filea.clear();
            assert_eq!(filea.read_at(0, &mut buffer), 0,);
            let mut str = String::new();
            use rand;
            // random digit
            for _ in 0..len {
                str.push(char::from('0' as u8 + rand::random::<u8>() % 10));
            }
            filea.write_at(0, str.as_bytes());
            let mut read_buffer = [0u8; 127];
            let mut offset = 0usize;
            let mut read_str = String::new();
            loop {
                let len = filea.read_at(offset, &mut read_buffer);
                if len == 0 {
                    break;
                }
                offset += len;
                read_str.push_str(core::str::from_utf8(&read_buffer[..len]).unwrap());
            }
            assert_eq!(str, read_str);
        };

        random_str_test(4 * BLOCK_SZ);
        random_str_test(8 * BLOCK_SZ + BLOCK_SZ / 2);
        random_str_test(100 * BLOCK_SZ);
        random_str_test(70 * BLOCK_SZ + BLOCK_SZ / 7);
        random_str_test((12 + 128) * BLOCK_SZ);
        random_str_test(400 * BLOCK_SZ);
        random_str_test(1000 * BLOCK_SZ);
        random_str_test(2000 * BLOCK_SZ);

        Ok(())
    }

    fn read_string(file: &Arc<Inode>) -> String {
        let mut read_buffer = [0u8; 512];
        let mut offset = 0usize;
        let mut read_str = String::new();
        loop {
            let len = file.read_at(offset, &mut read_buffer);
            if len == 0 {
                break;
            }
            offset += len;
            read_str.push_str(core::str::from_utf8(&read_buffer[..len]).unwrap());
        }
        read_str
    }

    fn tree(inode: &Arc<Inode>, name: &str, depth: usize) {
        if depth > 1 {
            for _ in 0..depth - 1 {
                print!("    ");
            }
        }
        if depth == 0 {
            println!("{}", name);
        } else {
            println!(
                "+-- {} f:{} {}B {}KB",
                name,
                inode.is_file(),
                inode.get_size(),
                inode.get_size() / 1024
            );
        }
        for f in inode.ls() {
            // skip "." and ".."
            if matches!(f.as_str(), "." | "..") {
                continue;
            }
            let child = inode
                .find(&f)
                .expect(&format!("{f} in `ls {name}`(d={depth}) but cannot find"));
            tree(&child, &f, depth + 1);
        }
    }

    #[test]
    fn efs_dir_test() -> std::io::Result<()> {
        let block_file = Arc::new(BlockFile(Mutex::new({
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open("target/fs.img")?;
            f.set_len(8192 * 512).unwrap();
            f
        })));
        EasyFileSystem::create(block_file.clone(), 4096, 1);
        let efs = EasyFileSystem::open(block_file.clone());
        let root = Arc::new(EasyFileSystem::root_inode(&efs));
        root.create("f1");
        root.create("f2");

        let d1 = root.create_dir("d1").unwrap();

        let f3 = d1.create("f3").unwrap();
        let d2 = d1.create_dir("d2").unwrap();

        let f4 = d2.create("f4").unwrap();
        tree(&root, "/", 0);

        let f3_content = "3333333";
        let f4_content = "4444444444444444444";
        f3.write_at(0, f3_content.as_bytes());
        f4.write_at(0, f4_content.as_bytes());

        assert_eq!(read_string(&d1.find("f3").unwrap()), f3_content);
        assert_eq!(read_string(&root.find("/d1/f3").unwrap()), f3_content);
        assert_eq!(read_string(&d2.find("f4").unwrap()), f4_content);
        assert_eq!(read_string(&d1.find("d2/f4").unwrap()), f4_content);
        assert_eq!(read_string(&root.find("/d1/d2/f4").unwrap()), f4_content);
        assert!(f3.find("whatever").is_none());
        Ok(())
    }

    #[test]
    fn efs_dir_dot_test() -> std::io::Result<()> {
        let block_file = Arc::new(BlockFile(Mutex::new({
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open("target/fs.img")?;
            f.set_len(8192 * 512).unwrap();
            f
        })));
        EasyFileSystem::create(block_file.clone(), 4096, 1);
        let efs = EasyFileSystem::open(block_file.clone());
        let root = Arc::new(EasyFileSystem::root_inode(&efs));

        root.create("file0");
        root.create("file1");
        println!("ls /\n{:?}", root.ls());

        if let Some(root_dot) = root.find(".") {
            println!("ls /.\n{:?}", root_dot.ls());
        }
        if let Some(root_ddot) = root.find("..") {
            println!("ls /..\n{:?}", root_ddot.ls());
            if let Some(ddot_of_root_ddot) = root_ddot.find("..") {
                println!("ls /../..\n{:?}", ddot_of_root_ddot.ls());
            }
        }
        if let Some(d) = root.find("./.././..") {
            println!("ls*(at /) ./.././..\n{:?}", d.ls());
        }

        println!("\ncreate dir0");
        let dir0 = root.create_dir("dir0").unwrap();
        dir0.create("dir0_file0");
        println!("ls dir0\n{:?}", dir0.ls());
        if let Some(dot) = dir0.find(".") {
            println!("ls dir0/.\n{:?}", dot.ls());
        }
        if let Some(ddot) = dir0.find("..") {
            println!("ls dir0/..\n{:?}", ddot.ls());
            if let Some(ddot_of_ddot) = ddot.find("..") {
                println!("ls dir0/../..\n{:?}", ddot_of_ddot.ls());
            }
        }
        if let Some(d) = dir0.find("./.././..") {
            println!("ls*(at dir0) ./.././..\n{:?}", d.ls()); // eqv. ls /
        }
        Ok(())
    }

    #[test]
    fn check_os_image() -> std::io::Result<()> {
        let block_file = Arc::new(BlockFile(Mutex::new({
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .open("target/os.img")?;
            f
        })));
        let efs = EasyFileSystem::open(block_file);
        let root = Arc::new(EasyFileSystem::root_inode(&efs));
        tree(&root, "/", 0);
        Ok(())
    }
}
