use std::{env, os::unix::prelude::CommandExt, process};

use libc::pid_t;

fn debugger(child: pid_t) {
    println!("debugging {child}")
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("not enough arguments");
        process::exit(1);
    }
    // split off program (debugger itself)
    let (_, args) = args.split_at(1);
    let (prog, args) = args.split_at(1);
    let prog = &prog[0];

    let pid = unsafe { libc::fork() };
    if pid == 0 {
        let err = process::Command::new(prog).args(args).exec();
        eprintln!("could not execute program: {err}");
        process::exit(2);
    } else {
        debugger(pid)
    }
}
