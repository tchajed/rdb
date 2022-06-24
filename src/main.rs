use std::{env, process};

use rdb::{debugger, run_target};

fn main() {
    // skip the debugger in the arguments
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.is_empty() {
        eprintln!("not enough arguments");
        process::exit(1);
    }
    let prog = &args[0];
    let args = &args[1..];

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
        debugger(prog.to_str().unwrap(), pid)
    }
}
