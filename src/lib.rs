use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io,
    os::unix::process::CommandExt,
    process::{self, Stdio},
};

use libc::pid_t;

mod ptrace;

use ptrace::WaitStatus;
use rustyline::{error::ReadlineError, Editor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Breakpoint {
    target: ptrace::Target,
    addr: u64,
    saved_data: Option<u8>,
}

impl Breakpoint {
    // x86 int $3
    // https://www.felixcloutier.com/x86/intn:into:int3:int1
    const INT3_INSTR: u8 = 0xcc;

    pub fn new(target: ptrace::Target, addr: u64) -> Self {
        Self {
            target,
            addr,
            saved_data: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.saved_data.is_some()
    }

    pub fn enable(&mut self) {
        assert!(!self.enabled(), "breakpoint is already enabled");
        let old_data = unsafe { self.target.peekdata(self.addr) };
        let saved = (old_data & 0xff) as u8;
        self.saved_data = Some(saved);
        let new_data = (old_data & (!0xffu64)) | (Self::INT3_INSTR as u64);
        unsafe { self.target.pokedata(self.addr, new_data) };
    }

    pub fn disable(&mut self) {
        assert!(self.enabled(), "breakpoint is not enabled");
        let old_data = unsafe { self.target.peekdata(self.addr) };
        let new_data = (old_data & (!0xffu64)) | (self.saved_data.unwrap() as u64);
        unsafe { self.target.pokedata(self.addr, new_data) };
        self.saved_data = None;
    }
}

struct Dbg {
    target: ptrace::Target,
    breakpoints: HashMap<u64, Breakpoint>,
}

impl Dbg {
    fn new(target: pid_t) -> Self {
        Self {
            target: ptrace::Target::new(target),
            breakpoints: HashMap::new(),
        }
    }

    fn parse_line(line: &str) -> (&str, Vec<&str>) {
        let parts: Vec<_> = line.trim_start().split(' ').collect();
        let (cmd, args) = parts.split_first().unwrap_or((&"", &[]));
        (cmd, args.to_vec())
    }

    fn handle_command(&mut self, cmd: &str, args: Vec<&str>) {
        if cmd == "help" {
            println!("supported commands:");
            let commands = vec![
                ("continue", "resume execution"),
                ("break [hex_addr]", "create a breakpoint"),
                ("disable [hex_addr]", "delete a breakpoint"),
                ("register", "interact with registers"),
                ("quit", "exit debugger"),
            ];
            let width = commands.iter().map(|(cmd, _)| cmd.len()).max().unwrap();
            for (cmd, desc) in commands.iter() {
                println!("  {:1$} -- {desc}", console::style(cmd).bold(), width);
            }
            return;
        }
        if cmd == "continue" || cmd == "c" {
            if !args.is_empty() {
                eprintln!("unexpected arguments to continue");
                return;
            }
            self.continue_execution();
            return;
        }
        if cmd == "break" {
            if args.len() != 1 {
                eprintln!("invalid args");
                return;
            }
            let addr = u64::from_str_radix(args[0], 16).unwrap();
            self.set_breakpoint_at_address(addr);
            return;
        }
        if cmd == "disable" {
            if args.len() != 1 {
                eprintln!("invalid args");
                return;
            }
            let addr = u64::from_str_radix(args[0], 16).unwrap();
            self.disable_breakpoint_at_address(addr);
            return;
        }
        if cmd == "register" {
            if args.is_empty() {
                eprintln!("missing args to register");
            }
            if args[0] == "dump" {
                self.dump_registers();
                return;
            }
            eprintln!("invalid register command {}", args[0]);
        }
        eprintln!("unknown command {}", cmd);
    }

    fn continue_execution(&self) {
        unsafe { self.target.cont(0) };

        match self.target.wait() {
            WaitStatus::Exited { status } => {
                if status == 0 {
                    println!("program exited");
                } else {
                    eprintln!("debugee exited with status {status}");
                }
            }
            WaitStatus::Stopped { signal: s } => {
                if s == libc::SIGTRAP {
                    println!("stopped at breakpoint");
                } else if s == libc::SIGSEGV {
                    eprintln!("SIGSEGV in target");
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

        let mut breakpoint = Breakpoint::new(self.target, addr);
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

    fn dump_registers(&self) {
        let regs = unsafe { self.target.getregs() };
        for r in ptrace::REGS.iter() {
            let val = r.reg.get_reg(&regs);
            println!("{:8} 0x{:016x}", r.name, val);
        }
    }

    fn run(&mut self) {
        println!("debugging pid {}", self.target);

        if let WaitStatus::Exited { .. } = self.target.wait() {
            eprintln!("debugee exited");
        }

        let mut rl = Editor::<()>::new();

        loop {
            let readline = rl.readline("rdb> ");
            match readline {
                Ok(line) => {
                    rl.add_history_entry(line.as_str());
                    let (cmd, args) = Self::parse_line(&line);
                    if cmd == "quit" || cmd == "q" {
                        break;
                    }
                    self.handle_command(cmd, args)
                }
                Err(ReadlineError::Interrupted) => {}
                Err(ReadlineError::Eof) => break,
                Err(err) => {
                    eprintln!("error: {:?}", err);
                    break;
                }
            }
        }
    }
}

pub fn debugger(target: pid_t) {
    Dbg::new(target).run()
}

pub fn run_target(prog: &OsStr, args: &[OsString]) -> io::Error {
    unsafe { libc::personality(libc::ADDR_NO_RANDOMIZE as u64) };
    unsafe { ptrace::trace_me() }
    process::Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .exec()
}
