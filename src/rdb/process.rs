use crate::rdb::process_registers::{ProcRegisters, RegisterValue};
use crate::rdb::register_info::{
    Register, RegisterId, RegisterType, User, UserFpRegsStruct, UserRegsStruct,
};
use nix::errno::Errno;
use nix::fcntl::{fcntl, FcntlArg, FdFlag};
use nix::libc::{ptrace, PTRACE_PEEKUSER, PTRACE_POKEUSER, PTRACE_SETFPREGS};
use nix::sys::ptrace;
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{close, execvp, fork, pipe, read, write, ForkResult, Pid};
use std::ffi::CString;
use std::process;

pub struct Process {
    pid: Pid,
    terminate_on_end: bool,
    pub process_state: ProcessState,
    pub proc_registers: ProcRegisters,
}

#[derive(Copy, Clone)]
pub enum ProcessState {
    Stopped,
    Running,
    Exited,
    Terminated,
}

impl Drop for Process {
    fn drop(&mut self) {
        println!("Dropping: {}", self.pid);
        if let ProcessState::Running = self.process_state {
            if let Err(e) = kill(self.pid, Signal::SIGSTOP) {
                eprintln!("Failed to stop process {}: {}", self.pid, e);
            }
        }
        ptrace::detach(self.pid, None);
        kill(self.pid, Signal::SIGCONT);
        if !self.terminate_on_end {
            println!("Not killing: {}", self.pid);
        }
        if self.terminate_on_end {
            println!("killing process: {}", self.pid);
            if let Err(e) = kill(self.pid, Signal::SIGKILL) {
                eprintln!("Failed to kill process {}: {}", self.pid, e);
            } else {
                waitpid(self.pid, None);
            }
        }
    }
}

impl Process {
    pub fn new(pid: Pid, terminate_on_end: bool, process_state: ProcessState, data: User) -> Self {
        let proc_registers = ProcRegisters::new(data);
        Self {
            pid,
            terminate_on_end,
            process_state,
            proc_registers,
        }
    }
    pub fn pid(&self) -> Pid {
        self.pid
    }
    pub unsafe fn attach(pid_arg: &str) -> Result<Process, String> {
        let pid = pid_arg
            .parse::<i32>()
            .map_err(|_| "Invalid PID: not a valid number")?;

        if pid <= 0 {
            return Err("Invalid PID: must be positive".to_string());
        }

        ptrace::attach(Pid::from_raw(pid)).map_err(|e| format!("Failed to attach: {}", e))?;

        let process_state = ProcessState::Running;

        let terminate_on_end = false;

        let data = User::default_user();

        let process = Process::new(Pid::from_raw(pid), terminate_on_end, process_state, data);

        Ok(process)
    }
    pub fn launch(program_path: &str) -> Result<Process, String> {
        let (read_fd, write_fd) = pipe().map_err(|e| format!("pipe failed: {}", e))?;

        fcntl(&read_fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).ok();

        fcntl(&write_fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).ok();

        unsafe {
            let fork_res = fork();
            match fork_res {
                Ok(ForkResult::Parent { child }) => {
                    close(write_fd).ok(); //  we only want to read from the parent

                    let mut buffer: [u8; 256] = [0; 256];

                    let bytes_read = read(&read_fd, &mut buffer).unwrap_or(0);

                    close(read_fd).ok();

                    let pid = child.as_raw();
                    let process_state = ProcessState::Running;
                    let terminate_on_end = true;
                    let data = User::default_user();

                    let process =
                        Process::new(Pid::from_raw(pid), terminate_on_end, process_state, data);

                    if bytes_read > 0 {
                        drop(process);
                        let msg = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                        return Err(format!("Child process failed with error: {}", msg));
                    }

                    Ok(process)
                }
                Ok(ForkResult::Child) => {
                    close(read_fd).ok(); // we only want to write from the child

                    let traceme_res = ptrace::traceme();
                    if let Err(e) = traceme_res {
                        let _ = write(
                            &write_fd,
                            format!("Tracing child process failed: {}", e).as_bytes(),
                        );
                        eprintln!("Tracing child process failed: {}", e);
                        close(write_fd).ok();
                        process::exit(1);
                    }

                    let program_path_c =
                        CString::new(program_path).expect("Cstring conversion failed");
                    let exec_args = vec![program_path_c.clone()];
                    let exec_res = execvp(&program_path_c, &exec_args);

                    // if the exec in the above line works fine then we never write something to the pipe
                    // nor do we ever close it

                    if let Err(e) = exec_res {
                        let _ = write(
                            &write_fd,
                            format!("Tracing child process failed: {}", e).as_bytes(),
                        );
                        eprintln!("Exec failed: {}", e);
                        close(write_fd).ok();
                        process::exit(1);
                    }
                    unreachable!();
                }
                Err(e) => Err(format!("Fork failed: {}", e)),
            }
        }
    }
    pub fn dispatch_command(&mut self, command: String) {
        let args: Vec<&str> = command.split_whitespace().collect();
        let command = args[0];
        if "continue".starts_with(command) {
            self.resume();
            if let Err(e) = self.wait_on_signal() {
                process::exit(1);
            } // breakpoint// process stops again
        } else {
            eprintln!("unknown command: {}", command)
        }
    }
    fn resume(&mut self) {
        if let Err(e) = ptrace::cont(self.pid(), None) {
            eprintln!("Couldn't Continue: {}", e);
            process::exit(1);
        }
        self.process_state = ProcessState::Running;
    }
    fn wait_on_signal(&mut self) -> Result<WaitStatus, Errno> {
        let wait_res = waitpid(self.pid, None);
        match wait_res {
            Ok(status) => {
                self.process_state = ProcessState::Stopped;
                self.read_all_registers();
                self.proc_registers.data_.print_user();
                self.read_all_registers();
                self.proc_registers.data_.print_user();
                Ok(status)
            }
            Err(e) => {
                eprintln!("waitpid failed: {}", e);
                self.process_state = ProcessState::Terminated;
                Err(e)
            }
        }
    }
    pub unsafe fn write_to_user_by_register_id(&mut self, id: RegisterId, val: RegisterValue) {
        let register = Register::by_id(id);
        let user_bytes = self.proc_registers.write_register(register, val);
        if register.register_type == RegisterType::Fpr {
            self.write_fprs()
        } else {
            let offset = register.offset;

            let offset = offset & !0b111;

            let bytes = RegisterValue::from_bytes::<u64>(user_bytes.add(offset));
            println!("----------------------------------------writing to user---------------------------------------------------------");
            self.write_user(offset, bytes);
        }
    }

    fn write_user(&self, offset: usize, data: u64) {
        use nix::libc::{ptrace, PTRACE_POKEUSER};
        let result = unsafe { ptrace(PTRACE_POKEUSER, self.pid.as_raw(), offset, data) };
        if result < 0 {
            panic!("Couldnt write to user");
        }
    }

    fn write_fprs(&self) {
        use nix::libc::{ptrace, PTRACE_SETFPREGS};
        let fprs = &self.proc_registers.data_.i387;
        let result = unsafe {
            ptrace(
                PTRACE_SETFPREGS,
                self.pid.as_raw(),
                std::ptr::null_mut::<std::ffi::c_void>(),
                &fprs as *const _ as *const std::ffi::c_void,
            )
        };
        if result < 0 {
            panic!("Couldnt write to Floating point registers");
        }
    }

    fn write_gprs(&self) {
        use nix::libc::{ptrace, PTRACE_SETREGS};
        let gprs = &self.proc_registers.data_.regs;
        let result = unsafe {
            ptrace(
                PTRACE_SETREGS,
                self.pid.as_raw(),
                std::ptr::null_mut::<std::ffi::c_void>(),
                &gprs as *const _ as *const std::ffi::c_void,
            )
        };
        if result < 0 {
            panic!("Couldnt write to General purpose registers");
        }
    }
    pub fn read_all_registers(&mut self) -> Result<&str, &str> {
        use nix::libc::{ptrace as libc_ptrace, PTRACE_GETFPREGS};
        let regs_libc = ptrace::getregs(self.pid).map_err(|_| "Couldnt read GPR registers")?;
        self.proc_registers.data_.regs = UserRegsStruct {
            r15: regs_libc.r15,
            r14: regs_libc.r14,
            r13: regs_libc.r13,
            r12: regs_libc.r12,
            rbp: regs_libc.rbp,
            rbx: regs_libc.rbx,
            r11: regs_libc.r11,
            r10: regs_libc.r10,
            r9: regs_libc.r9,
            r8: regs_libc.r8,
            rax: regs_libc.rax,
            rcx: regs_libc.rcx,
            rdx: regs_libc.rdx,
            rsi: regs_libc.rsi,
            rdi: regs_libc.rdi,
            orig_rax: regs_libc.orig_rax,
            rip: regs_libc.rip,
            cs: regs_libc.cs,
            eflags: regs_libc.eflags,
            rsp: regs_libc.rsp,
            ss: regs_libc.ss,
            fs_base: regs_libc.fs_base,
            gs_base: regs_libc.gs_base,
            ds: regs_libc.ds,
            es: regs_libc.es,
            fs: regs_libc.fs,
            gs: regs_libc.gs,
        };

        let mut fpregs: UserFpRegsStruct = unsafe { std::mem::zeroed() };
        let result = unsafe {
            ptrace(
                PTRACE_GETFPREGS,
                self.pid.as_raw(),
                std::ptr::null_mut::<std::ffi::c_void>(),
                &mut fpregs as *mut _ as *mut std::ffi::c_void,
            )
        };
        if result < 0 {
            return Err("Couldnt read to Floating point registers");
        } else {
            self.proc_registers.data_.i387 = fpregs;
        }
        let dbrs_ids = [
            RegisterId::Dr0,
            RegisterId::Dr1,
            RegisterId::Dr2,
            RegisterId::Dr3,
            RegisterId::Dr4,
            RegisterId::Dr5,
            RegisterId::Dr6,
            RegisterId::Dr7,
        ];
        for (i, id) in dbrs_ids.iter().enumerate() {
            let register = Register::by_id(*id);
            Errno::clear();
            let data = unsafe {
                libc_ptrace(
                    PTRACE_PEEKUSER,
                    self.pid.as_raw(),
                    register.offset,
                    std::ptr::null_mut::<std::ffi::c_void>(),
                )
            };
            if Errno::last() != Errno::UnknownErrno {
                return Err("Couldnt read Debug registers");
            }
            self.proc_registers.data_.u_debugreg[i] = data as u64;
        }
        Ok("Read all registers")
    }
}
