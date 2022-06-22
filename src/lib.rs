use std::{
    ffi::{OsStr, OsString},
    io::{self, stdout, Error, Result, Write},
    os::unix::prelude::CommandExt,
    process::{self, Stdio},
};

use std::collections::HashMap;

use libc::pid_t;

mod ptrace {
    use libc::{c_uint, pid_t};

    const TRACEME: c_uint = 0;
    const PEEKDATA: c_uint = 2;
    const POKEDATA: c_uint = 5;
    const CONT: c_uint = 7;

    pub unsafe fn trace_me() {
        libc::ptrace(TRACEME);
    }

    pub unsafe fn cont(pid: pid_t, signal: c_uint) {
        libc::ptrace(CONT, pid, 0, signal);
    }

    pub unsafe fn peekdata(pid: pid_t, addr: u64) -> u64 {
        libc::ptrace(PEEKDATA, pid, addr) as u64
    }

    pub unsafe fn pokedata(pid: pid_t, addr: u64, data: u64) {
        libc::ptrace(POKEDATA, pid, addr, data);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum WaitStatus {
    Exited(u8),
    Signaled(u32),
    Stopped(u32),
}

impl From<libc::c_int> for WaitStatus {
    fn from(stat_val: libc::c_int) -> Self {
        if libc::WIFEXITED(stat_val) {
            WaitStatus::Exited(libc::WEXITSTATUS(stat_val) as u8)
        } else if libc::WIFSIGNALED(stat_val) {
            WaitStatus::Signaled(libc::WTERMSIG(stat_val) as u32)
        } else if libc::WIFSTOPPED(stat_val) {
            WaitStatus::Stopped(libc::WSTOPSIG(stat_val) as u32)
        } else {
            panic!("unexpected wait status");
        }
    }
}

fn wait(pid: pid_t) -> WaitStatus {
    let mut status = 0;
    unsafe { libc::waitpid(pid, &mut status, 0) };
    return WaitStatus::from(status);
}

struct Breakpoint {
    child: pid_t,
    addr: u64,
    saved_data: Option<u8>,
}

impl Breakpoint {
    // x86 int $3
    const INT_INSTR: u8 = 0xcc;

    pub fn new(child: pid_t, addr: u64) -> Self {
        Self {
            child,
            addr,
            saved_data: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.saved_data.is_some()
    }

    pub fn enable(&mut self) {
        assert!(!self.enabled(), "breakpoint is already enabled");
        let old_data = unsafe { ptrace::peekdata(self.child, self.addr) };
        let saved = (old_data & 0xff) as u8;
        self.saved_data = Some(saved);
        let new_data = (old_data & (!0xff)) | (Self::INT_INSTR as u64);
        unsafe { ptrace::pokedata(self.child, self.addr, new_data) };
    }

    pub fn disable(&mut self) {
        assert!(self.enabled(), "breakpoint is not enabled");
        let old_data = unsafe { ptrace::peekdata(self.child, self.addr) };
        let new_data = (old_data & (!0xff)) | (self.saved_data.unwrap() as u64);
        unsafe { ptrace::pokedata(self.child, self.addr, new_data) };
    }
}

struct Dbg {
    child: pid_t,
    breakpoints: HashMap<u64, Breakpoint>,
}

impl Dbg {
    fn new(child: pid_t) -> Self {
        Self {
            child,
            breakpoints: HashMap::new(),
        }
    }

    fn wait(&self) -> WaitStatus {
        wait(self.child)
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

        match self.wait() {
            WaitStatus::Exited(status) => {
                if status == 0 {
                    println!("program exited");
                } else {
                    eprintln!("debugee exited with status {status}");
                }
            }
            _ => {}
        }
    }

    fn prompt() {
        print!("rdb> ");
        stdout().flush().unwrap();
    }

    fn run(&mut self) -> Result<()> {
        println!("debugging pid {}", self.child);

        match self.wait() {
            WaitStatus::Exited(_) => {
                eprintln!("debugee exited");
            }
            _ => {}
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
