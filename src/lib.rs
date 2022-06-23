use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::{self, stdout, Error, Result, Write},
    os::unix::process::CommandExt,
    process::{self, Stdio},
};

use libc::pid_t;

mod ptrace;

use ptrace::WaitStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Breakpoint {
    child: ptrace::Target,
    addr: u64,
    saved_data: Option<u8>,
}

impl Breakpoint {
    // x86 int $3
    const INT_INSTR: u8 = 0xcc;

    pub fn new(child: ptrace::Target, addr: u64) -> Self {
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
        let old_data = unsafe { self.child.peekdata(self.addr) };
        let saved = (old_data & 0xff) as u8;
        self.saved_data = Some(saved);
        let new_data = (old_data & (!0xff)) | (Self::INT_INSTR as u64);
        unsafe { self.child.pokedata(self.addr, new_data) };
    }

    pub fn disable(&mut self) {
        assert!(self.enabled(), "breakpoint is not enabled");
        let old_data = unsafe { self.child.peekdata(self.addr) };
        let new_data = (old_data & (!0xff)) | (self.saved_data.unwrap() as u64);
        unsafe { self.child.pokedata(self.addr, new_data) };
    }
}

struct Dbg {
    child: ptrace::Target,
    breakpoints: HashMap<u64, Breakpoint>,
}

impl Dbg {
    fn new(child: pid_t) -> Self {
        Self {
            child: ptrace::Target::new(child),
            breakpoints: HashMap::new(),
        }
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
        if cmd == &"break" {
            if args.len() != 1 {
                eprintln!("invalid args");
                return;
            }
            let addr = u64::from_str_radix(args[0], 16).unwrap();
            self.set_breakpoint_at_address(addr);
        }
        if cmd == &"disable" {
            if args.len() != 1 {
                eprintln!("invalid args");
                return;
            }
            let addr = u64::from_str_radix(args[0], 16).unwrap();
            self.disable_breakpoint_at_address(addr);
        }
    }

    fn continue_execution(&self) {
        unsafe { self.child.cont(0) };

        match self.child.wait() {
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

    fn set_breakpoint_at_address(&mut self, addr: u64) {
        if self.breakpoints.contains_key(&addr) {
            eprintln!("already have a breakpoint at 0x{:x}", addr);
            return;
        }

        let mut breakpoint = Breakpoint::new(self.child, addr);
        breakpoint.enable();
        self.breakpoints.insert(addr, breakpoint);
    }

    fn disable_breakpoint_at_address(&mut self, addr: u64) {
        match self.breakpoints.remove(&addr) {
            None => {}
            Some(mut bp) => {
                bp.disable();
            }
        }
    }

    fn prompt() {
        print!("rdb> ");
        stdout().flush().unwrap();
    }

    fn run(&mut self) -> Result<()> {
        println!("debugging pid {}", self.child);

        match self.child.wait() {
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
    unsafe { libc::personality(libc::ADDR_NO_RANDOMIZE as u64) };
    unsafe { ptrace::trace_me() }
    process::Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .exec()
}
