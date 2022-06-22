use std::{
    ffi::{OsStr, OsString},
    io::Error,
    os::unix::prelude::CommandExt,
    process,
};

use libc::pid_t;

pub fn debugger(child: pid_t) {
    println!("debugging {child}")
}

pub fn run_child(prog: &OsStr, args: &[OsString]) -> Error {
    process::Command::new(prog).args(args).exec()
}
