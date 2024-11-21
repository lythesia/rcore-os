#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user_lib;

use alloc::{string::String, vec::Vec};
use user_lib::{
    chdir, close, console::getchar, dup, exec, fork, getcwd, open, pipe, waitpid, OpenFlags,
};

const BS: u8 = 0x08;
const LF: u8 = 0x0a;
const CR: u8 = 0x0d;
const DL: u8 = 0x7f;

const PROMPT: &'static str = ">> ";

#[derive(Debug)]
struct ProcessArguments {
    input: String,
    output: String,
    args: Vec<String>,
    args_addr: Vec<*const u8>,
}

impl ProcessArguments {
    pub fn new(cmd: &str) -> Self {
        let mut args: Vec<_> = cmd
            .split_ascii_whitespace()
            .map(|arg| {
                let mut s = String::from(arg);
                s.push('\0');
                s
            })
            .collect();

        // parse redirect input
        let mut input = String::new();
        if let Some((idx, _)) = args
            .iter()
            .enumerate()
            .find(|(_, arg)| arg.as_str() == "<\0")
        {
            input = args[idx + 1].clone();
            args.drain(idx..=idx + 1); // remove "<\0"
        }

        // parse redirect output
        let mut output = String::new();
        if let Some((idx, _)) = args
            .iter()
            .enumerate()
            .find(|(_, arg)| arg.as_str() == ">\0")
        {
            output = args[idx + 1].clone();
            args.drain(idx..=idx + 1); // remove ">\0"
        }

        // exec args
        let mut args_addr = args.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
        args_addr.push(0 as *const u8);

        Self {
            input,
            output,
            args,
            args_addr,
        }
    }
}

#[no_mangle]
fn main() -> i32 {
    println!("Rust user shell");
    let mut line: String = String::new();
    loop {
        line.clear();
        print!("{}", PROMPT);
        'repl: loop {
            let c = getchar();
            match c {
                LF | CR => {
                    println!("");
                    let input = line.trim();
                    if input.is_empty() {
                        break 'repl;
                    }

                    // shell builtin
                    {
                        let (cmd, args_str) = input
                            .split_once(|ch: char| ch.is_ascii_whitespace())
                            .unwrap_or((input, ""));
                        let args = args_str
                            .split_ascii_whitespace()
                            .map(|arg| {
                                let mut s = String::from(arg);
                                s.push('\0');
                                s
                            })
                            .collect::<Vec<_>>();
                        match exec_builtin(cmd, &args) {
                            Ok(_) => break 'repl,
                            Err(e) => {
                                match e {
                                    UNKNOWN_BUILTIN => {} // continue to exec
                                    s => {
                                        println!("{}", s);
                                        break 'repl;
                                    }
                                }
                            }
                        }
                    }

                    let splited = input.split('|').collect::<Vec<_>>();
                    let process_arguments_list = splited
                        .iter()
                        .map(|&cmd| ProcessArguments::new(cmd))
                        .collect::<Vec<_>>();
                    let mut valid = true;
                    // handle pipe chain
                    for (i, process_args) in process_arguments_list.iter().enumerate() {
                        if i == 0 {
                            // head prog not allow >, coz it pipes to next
                            if !process_args.output.is_empty() {
                                valid = false;
                            }
                        } else if i == process_arguments_list.len() - 1 {
                            // tail prog not allow <, coz it pipes from prev
                            if !process_args.input.is_empty() {
                                valid = false;
                            }
                        } else if !(process_args.input.is_empty() && process_args.output.is_empty())
                        {
                            // intermediate progs not allow < nor >
                            valid = false;
                        }
                    }
                    // but if only one prog, always valid
                    if process_arguments_list.len() == 1 {
                        valid = true;
                    }
                    if !valid {
                        println!("Invalid command: Inputs/Outputs cannot be correctly binded!");
                        line.clear();
                        print!("{}", PROMPT);
                        continue;
                    }
                    // create pipes
                    let mut pipes_fd: Vec<[usize; 2]> = Vec::new();
                    if !process_arguments_list.is_empty() {
                        for _ in 0..process_arguments_list.len() - 1 {
                            let mut pipe_fd = [0usize; 2];
                            pipe(&mut pipe_fd);
                            pipes_fd.push(pipe_fd);
                        }
                    }
                    let mut children = Vec::new();
                    for (i, process_args) in process_arguments_list.iter().enumerate() {
                        // fork & exec
                        let pid = fork();
                        // child process
                        if pid == 0 {
                            let input = &process_args.input;
                            let output = &process_args.output;
                            let args = &process_args.args;
                            let args_addr = &process_args.args_addr;
                            // redirect input
                            if !input.is_empty() {
                                let input_fd = open(input.as_str(), OpenFlags::RDONLY);
                                if input_fd == -1 {
                                    println!("Error when opening file {}", input);
                                    return -4;
                                }
                                let input_fd = input_fd as usize;
                                close(0); // close stdin
                                assert_eq!(dup(input_fd), 0); // dup to 0
                                close(input_fd);
                            }
                            // redirect output
                            if !output.is_empty() {
                                let output_fd = open(
                                    output.as_str(),
                                    OpenFlags::WRONLY | OpenFlags::CREATE | OpenFlags::TRUNC,
                                );
                                if output_fd == -1 {
                                    println!("Error when opening file {}", output);
                                    return -4;
                                }
                                let output_fd = output_fd as usize;
                                close(1); // close stdout
                                assert_eq!(dup(output_fd), 1); // dup to 1
                                close(output_fd);
                            }
                            // recv input from prev prog
                            if i > 0 {
                                close(0);
                                let read_end = pipes_fd[i - 1][0];
                                assert_eq!(dup(read_end), 0);
                            }
                            // send output to next prog
                            if i < process_arguments_list.len() - 1 {
                                close(1);
                                let write_end = pipes_fd[i][1];
                                assert_eq!(dup(write_end), 1);
                            }
                            // close all pipe ends inherited from parent process
                            for pipe_fd in pipes_fd.iter() {
                                close(pipe_fd[0]);
                                close(pipe_fd[1]);
                            }
                            // exec
                            if exec(&args[0], args_addr.as_slice()) == -1 {
                                println!("[shell] cannot exec: {}", args[0]);
                                return -4;
                            }
                            unreachable!()
                        }
                        // shell process
                        else {
                            children.push(pid);
                        }
                    }
                    // close all pipe ends in shell process
                    for pipe_fd in pipes_fd.iter() {
                        close(pipe_fd[0]);
                        close(pipe_fd[1]);
                    }
                    // wait all progs
                    let mut exit_code = 0;
                    for pid in children {
                        let exit_pid = waitpid(pid as usize, &mut exit_code);
                        assert_eq!(pid, exit_pid);
                        // println!("[shell] Process: pid={} exit_code={}", pid, exit_code);
                    }
                    break 'repl;
                }
                BS | DL => {
                    if !line.is_empty() {
                        print!("{}", BS as char);
                        print!(" ");
                        print!("{}", BS as char);
                        line.pop();
                    }
                }
                _ => {
                    print!("{}", c as char);
                    line.push(c as char);
                }
            }
        }
    }
}

const UNKNOWN_BUILTIN: &'static str = "unknown builtin command!";
const WRONG_NUM_ARGS: &'static str = "wrong number of args!";
fn exec_builtin(cmd: &str, args: &[String]) -> Result<(), &'static str> {
    match cmd.trim_end_matches('\0') {
        "cd" => {
            let path = match args.len() {
                0 => "/\0",
                1 => args[0].as_str(),
                _ => return Err(WRONG_NUM_ARGS),
            };
            if chdir(path) == -1 {
                return Err("Error cd");
            }
        }
        "pwd" => {
            let mut path = [0; 256];
            if getcwd(&mut path[..]) == -1 {
                return Err("Error pwd");
            }
            println!("{}", core::str::from_utf8(&path).unwrap());
        }
        _ => return Err(UNKNOWN_BUILTIN),
    }
    Ok(())
}
