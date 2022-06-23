use std::{env, process};

use rdb::{debugger, run_target};

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("not enough arguments");
        process::exit(1);
    }
    // first argument is debugger itself
    let prog = &args[1];
    let args = &args[2..];

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        eprintln!("could not fork");
        process::exit(2);
    }
    if pid == 0 {
        let err = run_target(prog, args);
        eprintln!("could not execute program: {err}");
        process::exit(2);
    } else {
        debugger(pid)
    }
}
