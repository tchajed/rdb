use enum_iterator::{all, Sequence};
use std::{fmt::Display, io, mem::MaybeUninit};

use libc::{c_long, c_uint, pid_t, user_regs_struct};

pub fn trace_me() {
    unsafe { libc::ptrace(libc::PTRACE_TRACEME) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target(pid_t);

impl Target {
    pub fn pid(&self) -> pid_t {
        self.0
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Sequence)]
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

impl Reg {
    /// Get the lower-case name of the register.
    pub fn name(&self) -> String {
        // re-use the Debug instance
        format!("{:?}", self).to_ascii_lowercase()
    }
}

impl TryFrom<&str> for Reg {
    type Error = String;

    fn try_from(s: &str) -> std::result::Result<Self, Self::Error> {
        all::<Reg>()
            .find(|r| r.name() == s)
            .ok_or_else(|| "invalid register name".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::Reg;

    #[test]
    fn test_register_parse() {
        assert_eq!(Ok(Reg::Rbp), Reg::try_from("rbp"));
        assert_eq!(Ok(Reg::Orig_rax), Reg::try_from("orig_rax"));
        assert!(Reg::try_from("Rax").is_err());
        assert!(Reg::try_from("ebp").is_err());
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

type Result<T> = std::result::Result<T, io::Error>;

fn clear_errno() {
    unsafe {
        libc::__errno_location().write(0);
    }
}

fn get_errno() -> i32 {
    unsafe { libc::__errno_location().read() }
}

fn errno_result(r: c_long) -> Result<()> {
    if r < 0 {
        let errno = get_errno();
        Err(io::Error::from_raw_os_error(errno))
    } else {
        Ok(())
    }
}

fn ptrace(req: c_uint, pid: pid_t, addr: usize, data: usize) -> Result<()> {
    let r = unsafe { libc::ptrace(req, pid, addr, data) };
    errno_result(r)
}

impl Target {
    pub fn new(pid: pid_t) -> Self {
        Self(pid)
    }

    pub fn kill(self) -> Result<()> {
        let r = unsafe { libc::kill(self.0, libc::SIGKILL) };
        errno_result(r as c_long)
    }

    fn ptrace(&self, req: c_uint, addr: usize, data: usize) -> Result<()> {
        ptrace(req, self.0, addr, data)
    }

    pub fn cont(&self, signal: c_uint) -> Result<()> {
        self.ptrace(libc::PTRACE_CONT, 0, signal as usize)
    }

    pub fn peekdata(&self, addr: u64) -> Result<u64> {
        // need to do everything manually since the return value does not signal
        // errno (it could be -1 as actual data)
        clear_errno();
        let data = unsafe { libc::ptrace(libc::PTRACE_PEEKDATA, self.0, addr) as u64 };
        let err = get_errno();
        if err < 0 {
            return Err(io::Error::from_raw_os_error(err));
        }
        Ok(data)
    }

    pub fn pokedata(&self, addr: u64, data: u64) -> Result<()> {
        self.ptrace(libc::PTRACE_POKEDATA, addr as usize, data as usize)
    }

    pub fn wait(&self) -> Result<WaitStatus> {
        let mut status = 0;
        let r = unsafe { libc::waitpid(self.0, &mut status, 0) };
        errno_result(r as i64)?;
        Ok(status.into())
    }

    pub fn getregs(&self) -> Result<user_regs_struct> {
        let mut regs = MaybeUninit::<user_regs_struct>::uninit();
        let data = regs.as_mut_ptr() as usize;
        self.ptrace(libc::PTRACE_GETREGS, 0 /* addr is ignored */, data)?;
        unsafe { Ok(regs.assume_init()) }
    }

    pub fn getreg(&self, r: Reg) -> Result<u64> {
        let regs = self.getregs()?;
        Ok(r.get_reg(&regs))
    }

    fn setregs(&self, regs: &user_regs_struct) -> Result<()> {
        let data = regs as *const user_regs_struct as usize;
        self.ptrace(libc::PTRACE_SETREGS, 0 /* addr is ignored  */, data)
    }

    pub fn setreg(&self, r: Reg, val: u64) -> Result<()> {
        let mut regs = self.getregs()?;
        r.set_reg(&mut regs, val);
        self.setregs(&regs)
    }

    pub fn singlestep(&self) -> Result<()> {
        self.ptrace(
            libc::PTRACE_SINGLESTEP,
            0, /* ignored */
            0, /* ignored */
        )
    }

    pub fn getsiginfo(&self) -> Result<libc::siginfo_t> {
        let mut info = MaybeUninit::<libc::siginfo_t>::uninit();
        let data = info.as_mut_ptr() as usize;
        self.ptrace(libc::PTRACE_GETSIGINFO, 0 /* addr is ignored */, data)?;
        unsafe { Ok(info.assume_init()) }
    }
}
