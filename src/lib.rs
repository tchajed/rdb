use std::{
    ffi::{OsStr, OsString},
    io::{self, stdout, Error, Result, Write},
    os::unix::prelude::CommandExt,
    process::{self, Stdio},
};

use libc::pid_t;

mod ptrace {
    use libc::{c_uint, pid_t};

    const TRACEME: c_uint = 0;
    const CONT: c_uint = 7;

    pub unsafe fn trace_me() {
        libc::ptrace(TRACEME);
    }

    pub unsafe fn cont(pid: pid_t, signal: c_uint) {
        libc::ptrace(CONT, pid, 0, signal);
    }
}

struct Dbg {
    child: pid_t,
}

impl Dbg {
    fn new(child: pid_t) -> Self {
        Dbg { child }
    }

    fn handle_command(&mut self, line: String) {
        let parts: Vec<_> = line.split(' ').collect();
        if parts.is_empty() {
            return;
        }
        let cmd = &parts[0];
        let args = &parts[1..];
        if cmd == &"continue" || cmd == &"c" {
            if !args.is_empty() {
                eprintln!("unexpected arguments to continue");
                return;
            }
            self.continue_execution()
        }
    }

    fn continue_execution(&self) {
        unsafe { ptrace::cont(self.child, 0) };

        let mut status = 0;
        unsafe { libc::waitpid(self.child, &mut status, 0) };
        if libc::WIFEXITED(status) {
            let status = libc::WEXITSTATUS(status);
            if status == 0 {
                println!("program exited");
            } else {
                eprintln!("debugee exited with status {status}");
            }
        }
        if libc::WIFSIGNALED(status) {
            println!("child was signalled");
        }
    }

    fn prompt() {
        print!("rdb> ");
        stdout().flush().unwrap();
    }

    fn run(&mut self) -> Result<()> {
        println!("debugging pid {}", self.child);

        let mut status = 0;
        unsafe { libc::waitpid(self.child, &mut status, 0) };
        if libc::WIFEXITED(status) {
            eprintln!("debugee exited");
        }

        Self::prompt();
        for line in io::stdin().lines() {
            let line = line?;
            self.handle_command(line);
            Self::prompt();
        }
        Ok(())
    }
}

pub fn debugger(child: pid_t) -> Result<()> {
    Dbg::new(child).run()
}

pub fn run_child(prog: &OsStr, args: &[OsString]) -> Error {
    unsafe { ptrace::trace_me() }
    process::Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .exec()
}
