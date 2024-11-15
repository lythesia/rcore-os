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
        f.set_len(16 * 2048 * 512)?;
        f
    })));
    // 16MiB, at most 4095 files
    let efs = EasyFileSystem::create(block_file, 16 * 2048, 1);
    let root_inode = Arc::new(EasyFileSystem::root_inode(&efs));
    let apps = read_dir(opt.source.as_path())?
        .into_iter()
        .map(|dirent| {
            let mut fname = dirent?
                .file_name()
                .into_string()
                .map_err(|e| Error::other(format!("invalid os_string of file: {e:?}")))?;
            if let Some(dot) = fname.rfind('.') {
                fname.drain(dot..);
            }
            Ok(fname)
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    for app in apps {
        // load app data from host file system
        let path = opt.target.join(&app);
        let mut host_file = File::open(path)?;
        let mut all_data = Vec::new();
        host_file.read_to_end(&mut all_data)?;
        // create a file in easy-fs
        let inode = root_inode.create(&app).unwrap();
        // write data to easy-fs
        inode.write_at(0, &all_data);
    }
    // list apps
    println!("easy-fs-use >>>>");
    for app in root_inode.ls() {
        println!("easy-fs-use: + {}", app);
    }
    println!("easy-fs-use <<<<");
    Ok(())
}

fn main() {
    easy_fs_pack().expect("Error when packing easy-fs!");
}

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
