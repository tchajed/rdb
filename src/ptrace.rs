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

    pub fn getregs(&self) -> user_regs_struct {
        let mut regs = Self::default_user_regs_struct();
        unsafe {
            libc::ptrace(
                GETREGS,
                self.0,
                0, // addr is ignored
                &mut regs as *mut user_regs_struct as u64,
            )
        };
        regs
    }

    pub fn setregs(&self, regs: &user_regs_struct) {
        unsafe {
            libc::ptrace(
                SETREGS,
                self.0,
                0, // addr is ignored
                regs as *const user_regs_struct as u64,
            )
        };
    }
}
