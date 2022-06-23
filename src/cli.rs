use clap::{Parser, Subcommand};
use clap_num::maybe_hex;

use crate::ptrace::Reg;

fn parse_reg(s: &str) -> Result<Reg, String> {
    s.try_into()
}

#[derive(Parser)]
#[clap(
    subcommand_required = true,
    disable_help_subcommand = true,
    disable_help_flag = true
)]
pub struct Input {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// continue executing target
    Continue,
    /// set a breakpoint
    Break {
        #[clap(value_parser = maybe_hex::<u64>)]
        addr: u64,
    },
    /// delete a breakpoint
    Disable {
        #[clap(value_parser = maybe_hex::<u64>)]
        addr: u64,
    },
    /// interact with registers
    #[clap(subcommand)]
    Register(RegisterCommand),
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
        #[clap(value_parser = maybe_hex::<u64>)]
        val: u64,
    },
}