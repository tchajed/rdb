use clap::{IntoApp, Parser, Subcommand};

use crate::ptrace::Reg;

fn parse_reg(s: &str) -> Result<Reg, String> {
    s.try_into()
}

#[allow(clippy::from_str_radix_10)]
fn maybe_hex(s: &str) -> Result<u64, String> {
    let r = if let Some(s) = s.strip_prefix("0x") {
        u64::from_str_radix(s, 16)
    } else if let Some(s) = s.strip_prefix("0b") {
        u64::from_str_radix(s, 2)
    } else {
        u64::from_str_radix(s, 10)
    };
    r.map_err(|e| e.to_string())
}

#[derive(Parser)]
#[clap(
    subcommand_required = true,
    disable_help_subcommand = true,
    disable_help_flag = true
)]
struct Input {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakpointLoc {
    Addr { pc: u64 },
    Line { file: String, line: usize },
    Function { name: String },
}

impl BreakpointLoc {
    fn parse(value: &str) -> Result<Self, String> {
        if let Some(num) = value.strip_prefix("0x") {
            let pc = u64::from_str_radix(num, 16).map_err(|err| err.to_string())?;
            Ok(Self::Addr { pc })
        } else if let Some((file, line)) = value.split_once(':') {
            let line = line.parse::<usize>().map_err(|err| err.to_string())?;
            Ok(Self::Line {
                file: file.to_string(),
                line,
            })
        } else {
            Ok(Self::Function {
                name: value.to_string(),
            })
        }
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// continue executing target
    Continue,
    /// set a breakpoint
    Break {
        #[clap(value_parser = BreakpointLoc::parse)]
        loc: BreakpointLoc,
    },
    /// delete a breakpoint
    Disable {
        #[clap(value_parser = maybe_hex)]
        pc: u64,
    },
    /// interact with registers
    #[clap(subcommand)]
    Register(RegisterCommand),
    /// step over a single instruction
    Stepi,
    /// step out of the current function
    Finish,
    /// step into the next function
    Step,
    /// step over the next source line
    Next,
    /// lookup a symbol
    Symbol {
        #[clap(value_parser)]
        name: String,
    },
    /// print a backtrace
    Backtrace,
    /// exit debugger
    Quit,
    /// print help message
    Help,
}

#[derive(Subcommand)]
pub enum RegisterCommand {
    /// print values of all registers
    Dump,
    /// get register value
    Read {
        #[clap(value_parser = parse_reg)]
        reg: Reg,
    },
    /// set register value
    Write {
        #[clap(value_parser = parse_reg)]
        reg: Reg,
        #[clap(value_parser = maybe_hex)]
        val: u64,
    },
}

pub fn parse_line(line: &str) -> Result<Command, clap::Error> {
    let args = ["rdb"].iter().copied();
    let args = args.chain(line.split(' '));
    Input::try_parse_from(args).map(|input| input.command)
}

pub fn print_help() {
    _ = Input::command().print_long_help();
}
