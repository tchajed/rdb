#![allow(clippy::needless_return)]
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{self},
    os::unix::process::CommandExt,
    path::Path,
    process::{self, Stdio},
};

use libc::pid_t;
use object::Object;
use rustyline::{error::ReadlineError, Editor};

mod cli;
pub mod debugger;
mod dwarf;
mod ptrace;
mod source;

use cli::{BreakpointLoc, Command, RegisterCommand};
use debugger::Dbg;

fn handle_command(dbg: &mut Dbg, cmd: cli::Command) {
    match cmd {
        Command::Continue => dbg.continue_execution().expect("continue failed"),
        Command::Break { loc } => match loc {
            BreakpointLoc::Addr { pc } => dbg.set_user_breakpoint(pc),
            BreakpointLoc::Line { file, line } => {
                dbg.set_breakpoint_at_source_location(&file, line)
            }
            BreakpointLoc::Function { name } => dbg.set_breakpoint_at_function(&name),
        },
        Command::Disable { pc } => dbg.disable_user_breakpoint(pc),
        Command::Register(cmd) => match cmd {
            RegisterCommand::Dump => dbg.dump_registers(),
            RegisterCommand::Read { reg } => dbg.read_register(reg),
            RegisterCommand::Write { reg, val } => dbg.write_register(reg, val),
        },
        Command::Stepi => dbg.single_step(),
        Command::Finish => dbg.step_out(),
        Command::Step => dbg.step_in(),
        Command::Next => dbg.step_over(),
        Command::Quit => {
            return;
        }
        Command::Help => {
            cli::print_help();
        }
    }
}

fn interaction_loop(mut dbg: Dbg) {
    println!("debugging pid {}", dbg.target_pid());

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
                    Ok(cmd) => handle_command(&mut dbg, cmd),
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
    dbg.kill_target_if_running();
    _ = rl.save_history(".rdb.history");
}

pub fn debugger<P: AsRef<Path>>(path: P, target: pid_t) {
    let file = fs::File::open(&path).unwrap();
    let mmap = unsafe { memmap::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();
    if !object.is_little_endian() {
        panic!("only handling little endian");
    }
    let dbg = Dbg::new(object, target);
    interaction_loop(dbg);
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
