use std::fmt::Display;

use libc::{c_uint, pid_t, user_regs_struct};

const TRACEME: c_uint = 0;
const PEEKDATA: c_uint = 2;
const POKEDATA: c_uint = 5;
const CONT: c_uint = 7;
const GETREGS: c_uint = 12;
const SETREGS: c_uint = 13;

pub unsafe fn trace_me() {
    libc::ptrace(TRACEME);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target(pid_t);

impl Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WaitStatus {
    Exited { status: u8 },
    Signaled { signal: i32 },
    Stopped { signal: i32 },
}

impl From<libc::c_int> for WaitStatus {
    fn from(stat_val: libc::c_int) -> Self {
        if libc::WIFEXITED(stat_val) {
            WaitStatus::Exited {
                status: libc::WEXITSTATUS(stat_val) as u8,
            }
        } else if libc::WIFSIGNALED(stat_val) {
            WaitStatus::Signaled {
                signal: libc::WTERMSIG(stat_val),
            }
        } else if libc::WIFSTOPPED(stat_val) {
            WaitStatus::Stopped {
                signal: libc::WSTOPSIG(stat_val),
            }
        } else {
            panic!("unexpected wait status");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum Reg {
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rdi,
    Rsi,
    Rbp,
    Rsp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
    Rip,
    Rflags,
    Cs,
    Orig_rax,
    Fs_base,
    Gs_base,
    Fs,
    Gs,
    Ss,
    Ds,
    Es,
}

impl TryFrom<&str> for Reg {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        for desc in REGS.iter() {
            if desc.name == s {
                return Ok(desc.reg);
            }
        }
        Err(())
    }
}

impl Reg {
    fn user_regs_ptr<'a>(&self, regs: &'a mut user_regs_struct) -> &'a mut u64 {
        match self {
            Reg::Rax => &mut regs.rax,
            Reg::Rbx => &mut regs.rbx,
            Reg::Rcx => &mut regs.rcx,
            Reg::Rdx => &mut regs.rdx,
            Reg::Rdi => &mut regs.rdi,
            Reg::Rsi => &mut regs.rsi,
            Reg::Rbp => &mut regs.rbp,
            Reg::Rsp => &mut regs.rsp,
            Reg::R8 => &mut regs.r8,
            Reg::R9 => &mut regs.r9,
            Reg::R10 => &mut regs.r10,
            Reg::R11 => &mut regs.r11,
            Reg::R12 => &mut regs.r12,
            Reg::R13 => &mut regs.r13,
            Reg::R14 => &mut regs.r14,
            Reg::R15 => &mut regs.r15,
            Reg::Rip => &mut regs.rip,
            Reg::Rflags => &mut regs.eflags,
            Reg::Cs => &mut regs.cs,
            Reg::Orig_rax => &mut regs.orig_rax,
            Reg::Fs_base => &mut regs.fs_base,
            Reg::Gs_base => &mut regs.gs_base,
            Reg::Fs => &mut regs.fs,
            Reg::Gs => &mut regs.gs,
            Reg::Ss => &mut regs.ss,
            Reg::Ds => &mut regs.ds,
            Reg::Es => &mut regs.es,
        }
    }

    pub fn get_reg(&self, regs: &user_regs_struct) -> u64 {
        let mut regs = *regs;
        *self.user_regs_ptr(&mut regs)
    }

    pub fn set_reg(&self, regs: &mut user_regs_struct, val: u64) {
        *self.user_regs_ptr(regs) = val;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegDescriptor {
    reg: Reg,
    dwarf_r: usize,
    name: &'static str,
}

const fn desc(reg: Reg, dwarf_r: usize, name: &'static str) -> RegDescriptor {
    RegDescriptor { reg, dwarf_r, name }
}

const REGS: [RegDescriptor; 27] = [
    desc(Reg::R15, 5, "r15"),
    desc(Reg::R14, 4, "r14"),
    desc(Reg::R13, 3, "r13"),
    desc(Reg::R12, 2, "r12"),
    desc(Reg::Rbp, 6, "rbp"),
    desc(Reg::Rbx, 3, "rbx"),
    desc(Reg::R11, 1, "r11"),
    desc(Reg::R10, 0, "r10"),
    desc(Reg::R9, 9, "r9"),
    desc(Reg::R8, 8, "r8"),
    desc(Reg::Rax, 0, "rax"),
    desc(Reg::Rcx, 2, "rcx"),
    desc(Reg::Rdx, 1, "rdx"),
    desc(Reg::Rsi, 4, "rsi"),
    desc(Reg::Rdi, 5, "rdi"),
    desc(Reg::Orig_rax, 1, "orig_rax"),
    desc(Reg::Rip, 1, "rip"),
    desc(Reg::Cs, 1, "cs"),
    desc(Reg::Rflags, 9, "eflags"),
    desc(Reg::Rsp, 7, "rsp"),
    desc(Reg::Ss, 2, "ss"),
    desc(Reg::Fs_base, 8, "fs_base"),
    desc(Reg::Gs_base, 9, "gs_base"),
    desc(Reg::Ds, 3, "ds"),
    desc(Reg::Es, 0, "es"),
    desc(Reg::Fs, 4, "fs"),
    desc(Reg::Gs, 5, "gs"),
];

impl Target {
    pub fn new(pid: pid_t) -> Self {
        Self(pid)
    }

    pub unsafe fn cont(&self, signal: c_uint) {
        libc::ptrace(CONT, self.0, 0, signal);
    }

    pub unsafe fn peekdata(&self, addr: u64) -> u64 {
        libc::ptrace(PEEKDATA, self.0, addr) as u64
    }

    pub unsafe fn pokedata(&self, addr: u64, data: u64) {
        libc::ptrace(POKEDATA, self.0, addr, data);
    }

    pub fn wait(&self) -> WaitStatus {
        let mut status = 0;
        unsafe { libc::waitpid(self.0, &mut status, 0) };
        status.into()
    }

    fn default_user_regs_struct() -> user_regs_struct {
        user_regs_struct {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbp: 0,
            rbx: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rax: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            orig_rax: 0,
            rip: 0,
            cs: 0,
            eflags: 0,
            rsp: 0,
            ss: 0,
            fs_base: 0,
            gs_base: 0,
            ds: 0,
            es: 0,
            fs: 0,
            gs: 0,
        }
    }

    pub unsafe fn getregs(&self) -> user_regs_struct {
        let mut regs = Self::default_user_regs_struct();
        libc::ptrace(
            GETREGS,
            self.0,
            0, // addr is ignored
            &mut regs as *mut user_regs_struct as u64,
        );
        regs
    }

    pub unsafe fn setregs(&self, regs: &user_regs_struct) {
        libc::ptrace(
            SETREGS,
            self.0,
            0, // addr is ignored
            regs as *const user_regs_struct as u64,
        );
    }
}
