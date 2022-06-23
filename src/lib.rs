#![allow(clippy::needless_return)]
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io,
    os::unix::process::CommandExt,
    process::{self, Stdio},
};

use clap::{IntoApp, Parser};
use libc::pid_t;

mod ptrace;

use ptrace::{Reg, WaitStatus};
use rustyline::{error::ReadlineError, Editor};

mod cli;
use cli::*;

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
    running: bool,
    breakpoints: HashMap<u64, Breakpoint>,
}

impl Dbg {
    fn new(target: pid_t) -> Self {
        Self {
            target: ptrace::Target::new(target),
            running: true,
            breakpoints: HashMap::new(),
        }
    }

    fn parse_line(line: &str) -> Result<cli::Command, clap::Error> {
        let args = ["rdb"].iter().copied();
        let args = args.chain(line.split(' '));
        Input::try_parse_from(args).map(|input| input.command)
    }

    fn handle_command(&mut self, cmd: cli::Command) {
        match cmd {
            Command::Continue => self.continue_execution(),
            Command::Break { addr } => self.set_breakpoint_at_address(addr),
            Command::Disable { addr } => self.disable_breakpoint_at_address(addr),
            Command::Register(cmd) => match cmd {
                RegisterCommand::Dump => self.dump_registers(),
                RegisterCommand::Read { reg } => self.read_register(reg),
                RegisterCommand::Write { reg, val } => self.write_register(reg, val),
            },
            Command::Quit => {
                return;
            }
            Command::Help => {
                _ = Input::command().print_long_help();
            }
        }
    }

    fn continue_execution(&mut self) {
        self.step_over_breakpoint();

        unsafe { self.target.cont(0) };

        match self.target.wait() {
            WaitStatus::Exited { status } => {
                if status == 0 {
                    println!("program exited");
                } else {
                    eprintln!("debugee exited with status {status}");
                }
                self.running = false;
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
        let bp = self
            .breakpoints
            .entry(addr)
            .or_insert_with(|| Breakpoint::new(self.target, addr));
        if bp.enabled() {
            eprintln!("already have a breakpoint at 0x{:x}", addr);
            return;
        }
        bp.enable();
    }

    fn disable_breakpoint_at_address(&mut self, addr: u64) {
        match self.breakpoints.get_mut(&addr) {
            None => {
                eprintln!("no such breakpoint");
                return;
            }
            Some(bp) => {
                bp.disable();
            }
        }
    }

    fn dump_registers(&self) {
        let regs = unsafe { self.target.getregs() };
        let width = ptrace::REGS.iter().map(|r| r.name.len()).max().unwrap();
        for r in ptrace::REGS.iter() {
            let val = r.reg.get_reg(&regs);
            println!("{:width$} 0x{:016x}", r.name, val, width = width);
        }
    }

    fn read_register(&self, r: Reg) {
        let val = unsafe { self.target.getreg(r) };
        println!("0x{:x}", val);
    }

    fn write_register(&self, r: Reg, val: u64) {
        unsafe { self.target.setreg(r, val) };
    }

    fn get_pc(&self) -> u64 {
        unsafe { self.target.getreg(Reg::Rip) }
    }

    /// when stopped at a breakpoint, step past it
    fn step_over_breakpoint(&mut self) {
        let possible_bp_location = self.get_pc() - 1;
        if let Some(bp) = self.breakpoints.get_mut(&possible_bp_location) {
            if bp.enabled() {
                unsafe {
                    self.target.setreg(Reg::Rip, possible_bp_location);
                    bp.disable();
                    self.target.singlestep();
                    self.target.wait();
                    bp.enable();
                };
            }
        }
    }

    fn run(&mut self) {
        println!("debugging pid {}", self.target);

        if let WaitStatus::Exited { .. } = self.target.wait() {
            eprintln!("debugee exited");
        }

        let mut rl = Editor::<()>::new();
        _ = rl.load_history(".rdb.history");

        loop {
            let readline = rl.readline("rdb> ");
            match readline {
                Ok(line) => {
                    if line.is_empty() {
                        continue;
                    }
                    rl.add_history_entry(line.as_str());
                    match Self::parse_line(&line) {
                        Ok(Command::Quit) => break,
                        Ok(cmd) => self.handle_command(cmd),
                        Err(err) => {
                            eprintln!("{}", err);
                            continue;
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {}
                Err(ReadlineError::Eof) => break,
                Err(err) => {
                    eprintln!("error: {:?}", err);
                    break;
                }
            }
        }
        if self.running {
            // terminate child
            self.target.kill();
        }
        _ = rl.save_history(".rdb.history");
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
