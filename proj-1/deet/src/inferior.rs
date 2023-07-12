use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::signal::Signal::SIGTRAP;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::mem::size_of;
use std::os::unix::process::CommandExt;
use std::process::Child;
use std::process::Command;

use crate::dwarf_data::DwarfData;

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

pub struct Inferior {
    child: Child,
}

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(
        target: &str,
        args: &Vec<String>,
        break_points: &mut HashMap<usize, u8>,
    ) -> Option<Inferior> {
        let mut cmd = Command::new(target);
        cmd.args(args);
        unsafe {
            cmd.pre_exec(child_traceme);
        }
        let child = cmd.spawn().expect("Child process error");
        let mut inferior = Inferior { child };
        let status = inferior.wait(None).ok()?;

        for (addr, orig_byte) in break_points {
            // replacing the byte at breakpoint with the value 0xcc
            *orig_byte = inferior
                .write_byte(*addr, 0xcc)
                .expect("Error setting breakpoint");
        }

        if let Status::Stopped(signal::Signal::SIGTRAP, _signal) = status {
            Some(inferior)
        } else {
            None
        }
    }

    fn write_byte(&mut self, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> (8 * byte_offset)) & 0xff;
        let masked_word = word & !(0xff << (8 * byte_offset));
        let updated_word = masked_word | ((val as u64) << (8 * byte_offset));
        ptrace::write(
            self.pid(),
            aligned_addr as ptrace::AddressType,
            updated_word as *mut std::ffi::c_void,
        )?;
        Ok(orig_byte as u8)
    }

    /// commend 'contunie' after pause the debugger
    pub fn wake_up(&mut self, break_points: &HashMap<usize, u8>) -> Result<Status, nix::Error> {
        let pid = self.pid();
        let mut regs = ptrace::getregs(pid)?;
        let rip = regs.rip as usize;

        // check if inferior stopped at a breakpoint
        if let Some(orig_byte) = break_points.get(&(rip - 1)) {
            self.write_byte(rip - 1, *orig_byte)
                .expect("Error restoring original first byte of instruction");
            regs.rip = (rip - 1) as u64;
            ptrace::setregs(pid, regs).expect("Error rewingding instruction pointer");

            ptrace::step(pid, None)?;
            let status = self.wait(None)?;
            match status {
                Status::Stopped(SIGTRAP, _ins_ptr) => {
                    self.write_byte(rip - 1, 0xcc)
                        .expect("Error restoring 0xcc in breakpoint");
                }
                Status::Exited(exit_code) => {
                    return Ok(Status::Exited(exit_code));
                }
                Status::Signaled(signal) => {
                    return Ok(Status::Signaled(signal));
                }
                _ => {}
            }
        }

        ptrace::cont(pid, None)?;
        self.wait(None)
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }

    pub fn kill(&mut self) -> Result<(), std::io::Error> {
        println!("Killing running inferior (pid {})", self.pid());
        self.child.kill()
    }

    pub fn print_backtrace(&self, debug: &DwarfData) -> Result<(), nix::Error> {
        let mut instruction_ptr = ptrace::getregs(self.pid())?.rip as usize;
        let mut base_ptr = ptrace::getregs(self.pid())?.rbp as usize;
        loop {
            let line = DwarfData::get_line_from_addr(debug, instruction_ptr)
                .expect("Error getting line from %rip");
            let func = DwarfData::get_function_from_addr(debug, instruction_ptr)
                .expect("Error getting function from %rip");
            println!("{} {}", func, line);

            if func == "main" {
                break;
            }
            instruction_ptr =
                ptrace::read(self.pid(), (base_ptr + 8) as ptrace::AddressType)? as usize;
            base_ptr = ptrace::read(self.pid(), base_ptr as ptrace::AddressType)? as usize;
        }
        Ok(())
    }
}
