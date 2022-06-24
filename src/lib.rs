#![allow(clippy::needless_return)]
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs,
    io::{self, BufRead},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{self, Stdio},
};

use libc::pid_t;
use object::{Object, ObjectKind};
use regex::Regex;
use rustyline::{error::ReadlineError, Editor};

mod ptrace;
use ptrace::{Reg, WaitStatus};

mod cli;
use cli::{Command, RegisterCommand};

mod dwarf;
use dwarf::DbgInfo;

mod source;
use source::print_source;

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
        let old_data = self.target.peekdata(self.addr);
        let saved = (old_data & 0xff) as u8;
        self.saved_data = Some(saved);
        let new_data = (old_data & (!0xffu64)) | (Self::INT3_INSTR as u64);
        self.target.pokedata(self.addr, new_data);
    }

    pub fn disable(&mut self) {
        assert!(self.enabled(), "breakpoint is not enabled");
        let old_data = self.target.peekdata(self.addr);
        let new_data = (old_data & (!0xffu64)) | (self.saved_data.unwrap() as u64);
        self.target.pokedata(self.addr, new_data);
        self.saved_data = None;
    }
}

// taken from kernel
const SI_KERNEL: i32 = 128;
const TRAP_BRKPT: i32 = 1;
const TRAP_TRACE: i32 = 2;

fn display_code(si_code: i32) -> String {
    match si_code {
        SI_KERNEL => "SI_KERNEL".to_string(),
        TRAP_BRKPT => "TRAP_BRKPT".to_string(),
        TRAP_TRACE => "TRAP_TRACE".to_string(),
        _ => format!("{}", si_code),
    }
}

struct Dbg {
    target: ptrace::Target,
    load_addr: u64,
    #[allow(dead_code)]
    info: DbgInfo,
    running: bool,
    breakpoints: HashMap<u64, Breakpoint>,
}

impl Dbg {
    fn get_load_address(file: &object::File, target: pid_t) -> Result<u64, io::Error> {
        if file.kind() != ObjectKind::Dynamic {
            return Ok(0);
        }
        let f =
            fs::File::open(format!("/proc/{target}/maps")).expect("could not open memory mapping");
        let f = io::BufReader::new(f);
        let re = Regex::new(
            r"(?P<start>[0-9a-f]*)-([0-9a-f]*) (?P<mode>[^ ]*) (?P<offset>[0-9a-f]*) ([^ ]*) ([^ ]*) *(?P<path>.*)",
        ).unwrap();
        for line in f.lines() {
            let line = line?;
            if let Some(captures) = re.captures(&line) {
                let off = captures.name("offset").unwrap().as_str();
                let off = u64::from_str_radix(off, 16).expect("could not parse offset");
                if off == 0 {
                    let start = captures.name("start").unwrap().as_str();
                    let start = u64::from_str_radix(start, 16).expect("could not parse start");
                    return Ok(start);
                }
            }
        }
        panic!("could not parse map file")
    }

    fn new(file: object::File, pid: pid_t) -> Self {
        let info = DbgInfo::new(&file).expect("could not load dwarf file");
        let target = ptrace::Target::new(pid);
        target.wait();
        let load_addr = Self::get_load_address(&file, pid).expect("could not get load address");
        Self {
            target,
            load_addr,
            info,
            running: true,
            breakpoints: HashMap::new(),
        }
    }

    fn handle_command(&mut self, cmd: cli::Command) {
        match cmd {
            Command::Continue => self.continue_execution(),
            Command::Break { pc } => self.set_breakpoint_at_address(self.load_addr + pc),
            Command::Disable { pc } => self.disable_breakpoint_at_address(self.load_addr + pc),
            Command::Register(cmd) => match cmd {
                RegisterCommand::Dump => self.dump_registers(),
                RegisterCommand::Read { reg } => self.read_register(reg),
                RegisterCommand::Write { reg, val } => self.write_register(reg, val),
            },
            Command::Quit => {
                return;
            }
            Command::Help => {
                cli::print_help();
            }
        }
    }

    fn handle_sigtrap(&self, siginfo: libc::siginfo_t) {
        let code = siginfo.si_code;
        if code == SI_KERNEL || code == TRAP_BRKPT {
            let pc = self.get_pc() - 1;
            self.set_pc(pc);
            println!("hit breakpoint 0x{:x}", pc - self.load_addr);
            let offset_pc = pc - self.load_addr;
            let loc = self
                .info
                .source_for_pc(offset_pc)
                .expect("could not lookup source");
            if let Some(loc) = loc {
                let path: PathBuf = loc.file.unwrap().into();
                print_source(&path, loc.line.unwrap() as usize, 1);
            }
        } else if code == TRAP_TRACE {
            // from single-stepping
            return;
        } else {
            eprintln!("unknown SIGTRAP code {}", code);
        }
    }

    fn continue_execution(&mut self) {
        self.step_over_breakpoint();
        self.target.cont(0);
        let s = self.target.wait();

        if let WaitStatus::Exited { status } = s {
            if status == 0 {
                println!("program exited");
            } else {
                eprintln!("program exited with status {status}");
            }
            self.running = false;
        }

        let siginfo = self.target.getsiginfo();
        let signo = siginfo.si_signo;
        if signo == 0 {
            // no signal
            return;
        }
        if signo == libc::SIGTRAP {
            self.handle_sigtrap(siginfo);
        } else if signo == libc::SIGSEGV {
            println!("yay segfault: {}", display_code(siginfo.si_code));
        } else {
            println!("got signal {}", siginfo.si_signo);
        }
    }

    fn set_breakpoint_at_address(&mut self, addr: u64) {
        let bp = self
            .breakpoints
            .entry(addr)
            .or_insert_with(|| Breakpoint::new(self.target, addr));
        if bp.enabled() {
            eprintln!("already have a breakpoint at 0x{:x}", addr - self.load_addr);
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
        let regs = self.target.getregs();
        let width = ptrace::REGS.iter().map(|r| r.name.len()).max().unwrap();
        for r in ptrace::REGS.iter() {
            let val = r.reg.get_reg(&regs);
            println!("{:width$} 0x{:016x}", r.name, val, width = width);
        }
    }

    fn read_register(&self, r: Reg) {
        let val = self.target.getreg(r);
        println!("0x{:x}", val);
    }

    fn write_register(&self, r: Reg, val: u64) {
        self.target.setreg(r, val);
    }

    fn get_pc(&self) -> u64 {
        self.target.getreg(Reg::Rip)
    }

    fn set_pc(&self, pc: u64) {
        self.target.setreg(Reg::Rip, pc);
    }

    /// when stopped at a breakpoint, step past it
    fn step_over_breakpoint(&mut self) {
        let pc = self.get_pc();
        if pc == 0 {
            return;
        }
        if let Some(bp) = self.breakpoints.get_mut(&pc) {
            if bp.enabled() {
                bp.disable();
                self.target.singlestep();
                self.target.wait();
                bp.enable();
            }
        }
    }

    fn run(&mut self) {
        println!("debugging pid {}", self.target);

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
                    match cli::parse_line(&line) {
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

pub fn debugger<P: AsRef<Path>>(path: P, target: pid_t) {
    let file = fs::File::open(&path).unwrap();
    let mmap = unsafe { memmap::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();
    if !object.is_little_endian() {
        panic!("only handling little endian");
    }
    Dbg::new(object, target).run()
}

pub fn run_target(prog: &OsStr, args: &[OsString]) -> io::Error {
    unsafe { libc::personality(libc::ADDR_NO_RANDOMIZE as u64) };
    ptrace::trace_me();
    process::Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .exec()
}
