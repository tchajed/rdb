use std::fmt::Display;

use libc::{c_uint, pid_t};

const TRACEME: c_uint = 0;
const PEEKDATA: c_uint = 2;
const POKEDATA: c_uint = 5;
const CONT: c_uint = 7;

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
    Exited(u8),
    Signaled(u32),
    Stopped(u32),
}

impl From<libc::c_int> for WaitStatus {
    fn from(stat_val: libc::c_int) -> Self {
        if libc::WIFEXITED(stat_val) {
            WaitStatus::Exited(libc::WEXITSTATUS(stat_val) as u8)
        } else if libc::WIFSIGNALED(stat_val) {
            WaitStatus::Signaled(libc::WTERMSIG(stat_val) as u32)
        } else if libc::WIFSTOPPED(stat_val) {
            WaitStatus::Stopped(libc::WSTOPSIG(stat_val) as u32)
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
}
