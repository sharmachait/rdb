use crate::rdb::process_registers::{ProcRegisters, RegisterValue};
use crate::rdb::register_info::{
    str_to_int, Register, RegisterId, RegisterType, User, UserFpRegsStruct, UserRegsStruct,
};
use crate::rdb::stop_points::breakpoint_va::BreakpointVA;
use crate::rdb::stop_points::stoppoint_collection::{Stoppoint, StoppointCollection};
use crate::rdb::virtual_address::VirtAddr;
use nix::errno::Errno;
use nix::fcntl::{fcntl, FcntlArg, FdFlag};
use nix::libc::{personality, ADDR_NO_RANDOMIZE};
use nix::libc::{ptrace, PTRACE_PEEKUSER};
use nix::sys::ptrace;
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{close, execvp, fork, pipe, read, write, ForkResult, Pid};
use std::ffi::CString;
use std::os::fd::RawFd;
use std::process;

pub struct Process {
    pub breakpoint_id: usize,
    pid: Pid,
    terminate_on_end: bool,
    pub stop_points: StoppointCollection<BreakpointVA>,
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
            breakpoint_id: 0,
            pid,
            terminate_on_end,
            process_state,
            proc_registers,
            stop_points: StoppointCollection::default(),
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
    pub fn launch(
        program_path: &str,
        stdout_replacement: Option<RawFd>,
    ) -> Result<Process, String> {
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

                    if personality(ADDR_NO_RANDOMIZE as libc::c_ulong) < 0 {
                        let _ = write(
                            &write_fd,
                            "Failed to disable memory randomization".as_bytes(),
                        );
                        eprintln!("Failed to disable memory randomiazation");
                        close(write_fd).ok();
                        process::exit(1);
                    }

                    if let Some(fd) = stdout_replacement {
                        if libc::dup2(fd, libc::STDOUT_FILENO) < 0 {
                            panic!("stdout_replacement failed");
                        }
                    }

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
    pub unsafe fn dispatch_command(&mut self, command: String) {
        let args: Vec<&str> = command.split_whitespace().collect();
        let command = args[0];
        if "continue".starts_with(command) {
            self.resume();
            if let Err(e) = self.wait_on_signal() {
                process::exit(1);
            } // breakpoint// process stops again
        } else if "help".starts_with(command) {
            self.handle_help(args);
        } else if "register".starts_with(command) {
            self.handle_register(args);
        } else if "breakpoint".starts_with(command) {
            self.handle_breakpoint(args)
        } else if "step".starts_with(command) {
            println!("Stepping Over one instruction");
            self.step_one_instruction(args);
        } else {
            eprintln!("unknown command: {}", command)
        }
    }
    fn handle_breakpoint(&mut self, args: Vec<&str>) {
        if args.len() < 2 {
            self.handle_help(args);
            return;
        }

        let subcommand = args[1];
        if "list".starts_with(subcommand) {
            if self.stop_points.is_empty() {
                println!("No breakpoints set");
            } else {
                println!("Current Breakpoints: ");
                self.stop_points.for_each(|bp| {
                    println!(
                        "{}: address = {:#x}, {}",
                        bp.id(),
                        bp.address().addr(),
                        if bp.is_enabled() {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                });
            }
            return;
        }

        if args.len() < 3 {
            self.handle_help(args);
            return;
        }

        if "set".starts_with(subcommand) {
            let pid = self.pid();
            let address: Option<u64> = str_to_int::<u64>(args[2], 16);
            if address.is_none() {
                eprintln!("Breakpoint command expects address in hexadecimal");
                return;
            }
            let virt_addr = VirtAddr::with_addr(address.unwrap());
            let bp_res = self.add_breakpoint_at(virt_addr);
            if let Ok(bp) = bp_res {
                bp.enable(pid);
            } else {
                eprintln!("Not able to set BP");
            }
            return;
        }
        let id: Option<usize> = str_to_int::<usize>(args[2], 10);
        if id.is_none() {
            eprintln!("Command expects breakpoint id in base 10");
            return;
        }
        let id = id.unwrap();
        let bp = self.stop_points.get_by_id_mut(id);
        if bp.is_none() {
            eprintln!("Invalid id");
            return;
        }
        let bp = bp.unwrap();
        if "enable".starts_with(subcommand) {
            let res = bp.enable(self.pid);
            if let Err(e) = res {
                eprintln!("{}", e);
            }
        }
        if "disable".starts_with(subcommand) {
            let res = bp.disable(self.pid);
            if let Err(e) = res {
                eprintln!("{}", e);
            }
        }
        if "delete".starts_with(subcommand) {
            self.stop_points.remove_by_id(id, self.pid);
        }
    }
    fn handle_help(&self, args: Vec<&str>) {
        println!("=================================================================================================");

        if args.len() == 1 {
            self.print_general_help();
        } else {
            self.print_command_help(args[1]);
        }
    }
    fn print_general_help(&self) {
        println!("Available Commands:");
        println!("  continue  - Resume the debuggee");
        println!("  register  - Read and write to registers");
        println!("  breakpoint - Manage breakpoints");
        println!("  help      - Show this help message");
        println!();
        println!("Use 'help <command>' for detailed information about a specific command");
    }

    fn print_command_help(&self, command: &str) {
        match command {
            c if "register".starts_with(c) => {
                println!("Register Commands:");
                println!("  register read                - Show all registers");
                println!("  register read all            - Show all registers");
                println!("  register read <register>     - Show specific register value");
                println!("  register write <reg> <value> - Write value to register");
                println!();
                println!("Examples:");
                println!("  register read rax");
                println!("  register write rip 0x401000");
            }
            c if "breakpoint".starts_with(c) => {
                println!("Breakpoint Commands:");
                println!("  breakpoint list              - List all breakpoints");
                println!("  breakpoint set <address>     - Set breakpoint at address (hex)");
                println!("  breakpoint enable <id>       - Enable breakpoint by ID");
                println!("  breakpoint disable <id>      - Disable breakpoint by ID");
                println!("  breakpoint delete <id>       - Delete breakpoint by ID");
                println!();
                println!("Examples:");
                println!("  breakpoint set 0x401000");
                println!("  breakpoint enable 1");
                println!("  breakpoint delete 2");
            }
            c if "continue".starts_with(c) => {
                println!("Continue Command:");
                println!("  continue - Resume execution of the debuggee");
                println!();
                println!("The process will run until it hits a breakpoint, receives a signal,");
                println!("or terminates.");
            }
            _ => {
                eprintln!("Unknown command: {}", command);
                println!();
                self.print_general_help();
            }
        }
    }
    unsafe fn handle_register(&mut self, args: Vec<&str>) {
        if args.len() < 2 {
            self.handle_help(args);
            return;
        }

        let subcommand = args[1]; // read / write
        if "read".starts_with(subcommand) {
            self.handle_register_read(args);
        } else if "write".starts_with(subcommand) {
            self.handle_register_write(args);
        } else {
            let command = "register";
            println!("Unsupported command: {} {}", command, subcommand);
        }
    }

    unsafe fn handle_register_write(&mut self, args: Vec<&str>) {
        if args.len() != 4 {
            println!("Unsupported command: {} {}", args[0], args[1]);
            self.handle_help(args);
            return;
        }

        let register = Register::by_name(args[2]);
        if let None = register {
            println!("Invalid Register namer: {}", args[2]);
            return;
        }

        let register = register.unwrap();
        let value = register.parse_value(args[3]);
        if let Err(e) = value {
            println!("{}", e);
            return;
        }
        let value = value.unwrap();

        let res = self.write_to_user_by_register_id(register.id, value);

        if let Err(e) = res {
            eprintln!("{}", e);
        }
    }

    unsafe fn handle_register_read(&mut self, args: Vec<&str>) {
        if args.len() == 2 || (args.len() == 3 && args[2] == "all") {
            let res = self.read_all_registers();
            if let Err(e) = res {
                eprintln!("{}", e);
                return;
            }
            self.proc_registers.data_.print_user();
        } else if args.len() == 3 {
            let register = Register::by_name(args[2]);
            if let None = register {
                println!("Invalid Register namer: {}", args[2]);
                return;
            }
            let register = register.unwrap();
            let res = self.read_all_registers();
            if let Err(e) = res {
                eprintln!("{}", e);
                return;
            }

            let val = self.proc_registers.get_register_val_by_id(register.id);

            if let Err(e) = val {
                println!("{}", e);
                return;
            }

            let val = val.unwrap();
            println!("{}:           {}", args[2], val);
        } else {
            println!("Unsupported command: {} {}", args[0], args[1]);
            self.handle_help(args);
            return;
        }
    }

    fn resume(&mut self) {
        let pid = self.pid();
        let rip_va = unsafe { self.get_instruction_pointer_va() };
        match rip_va {
            Ok(va) => {
                // if we happen to resume at a breakpoint we will restore it step over it re enable
                // it and then continue as we were doing before
                let pre_rip_va = VirtAddr::with_addr(va.addr() - 1);
                if self.stop_points.is_stoppoint_enabled_by_address(&va) {
                    let bp = self.stop_points.get_by_address_mut(&va).unwrap();
                    let disable_res = bp.disable(pid);
                    if let Err(e) = disable_res {
                        eprintln!("Disabling breakpoint failed while stepping over: {}", e);
                        process::exit(1);
                    }
                    // single step over the instruction we just restored
                    if let Err(e) = ptrace::step(pid, None) {
                        eprintln!("Failed to single step: {}", e);
                        process::exit(1);
                    }

                    // wait for the single step to complete
                    if let Err(e) = waitpid(pid, None) {
                        eprintln!("waitpid failed: {}", e);
                        process::exit(1);
                    }

                    // re-enable the breakpoint
                    let bp = self.stop_points.get_by_address_mut(&va).unwrap();
                    if let Err(e) = bp.enable(pid) {
                        eprintln!("Re-enabling breakpoint failed: {}", e);
                        process::exit(1);
                    }
                }
            }
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
        if let Err(e) = ptrace::cont(self.pid(), None) {
            eprintln!("Couldn't Continue: {}", e);
            process::exit(1);
        }
        self.process_state = ProcessState::Running;
    }
    fn wait_on_signal(&mut self) -> Result<WaitStatus, &str> {
        let wait_res = waitpid(self.pid, None);
        match wait_res {
            Ok(status @ WaitStatus::Stopped(child_pid, signal)) => {
                self.process_state = ProcessState::Stopped;
                let res = self.read_all_registers();
                if let Err(e) = res {
                    eprintln!("{}", e);
                }
                let rip: VirtAddr = unsafe { self.get_instruction_pointer_va().map_err(|e| e)? };
                let rip_val = rip.addr();
                let intruction_beginning = rip_val - 1;
                let instruction_bgeinning_addr = VirtAddr::with_addr(intruction_beginning);
                if signal == Signal::SIGTRAP
                    && self
                        .stop_points
                        .is_stoppoint_enabled_by_address(&instruction_bgeinning_addr)
                {
                    unsafe {
                        self.set_instruction_pointer_va(instruction_bgeinning_addr);
                    }
                }

                Ok(status)
            }
            Ok(status @ WaitStatus::Exited(child_pid, code)) => {
                eprintln!("Process exited with code {}", code);
                self.process_state = ProcessState::Exited;
                Ok(status)
            }
            Ok(status @ WaitStatus::Signaled(child_pid, sig, core_dumped)) => {
                eprintln!("Process killed by signal {:?}", sig);
                self.process_state = ProcessState::Terminated;
                Ok(status)
            }
            Ok(status @ WaitStatus::Continued(_child_pid)) => {
                self.process_state = ProcessState::Running;
                Ok(status)
            }
            Ok(status @ WaitStatus::StillAlive) => {
                self.process_state = ProcessState::Running;
                Ok(status)
            }
            Ok(status) => {
                self.process_state = ProcessState::Stopped;
                let res = self.read_all_registers();
                if let Err(e) = res {
                    eprintln!("{}", e);
                }
                Ok(status)
            }
            Err(e) => {
                eprintln!("waitpid failed: {}", e);
                self.process_state = ProcessState::Terminated;
                Err("waitpid failed")
            }
        }
    }

    pub unsafe fn write_to_user_by_register_id(
        &mut self,
        id: RegisterId,
        val: RegisterValue,
    ) -> Result<&str, &str> {
        let register = Register::by_id(id);
        let user_bytes = self.proc_registers.write_register(register, val);
        if user_bytes == std::ptr::null_mut() {
            return Err("Couldnt write register.");
        }
        if register.register_type == RegisterType::Fpr {
            self.write_fprs()
        } else {
            let offset = register.offset;
            let offset = offset & !0b111;
            let bytes = RegisterValue::from_bytes::<u64>(user_bytes.add(offset));
            self.write_user(offset, bytes)
        }
    }

    fn write_user(&self, offset: usize, data: u64) -> Result<&str, &str> {
        use nix::libc::{ptrace, PTRACE_POKEUSER};
        let result = unsafe { ptrace(PTRACE_POKEUSER, self.pid.as_raw(), offset, data) };
        if result < 0 {
            return Err("Couldnt write to user");
        }
        return Ok("Wrote to user");
    }

    fn write_fprs(&self) -> Result<&str, &str> {
        use nix::libc::{ptrace, PTRACE_SETFPREGS};
        let fprs = &self.proc_registers.data_.i387;
        let result = unsafe {
            ptrace(
                PTRACE_SETFPREGS,
                self.pid.as_raw(),
                std::ptr::null_mut::<std::ffi::c_void>(),
                fprs as *const _ as *const std::ffi::c_void,
            )
        };
        if result < 0 {
            Err("Couldnt write to Floating point registers")
        } else {
            Ok("Register updated")
        }
    }

    fn write_gprs(&self) -> Result<&str, &str> {
        use nix::libc::{ptrace, PTRACE_SETREGS};
        let gprs = &self.proc_registers.data_.regs;
        let result = unsafe {
            ptrace(
                PTRACE_SETREGS,
                self.pid.as_raw(),
                std::ptr::null_mut::<std::ffi::c_void>(),
                gprs as *const _ as *const std::ffi::c_void,
            )
        };
        if result < 0 {
            Err("Couldnt write to General purpose registers")
        } else {
            Ok("Register Updated")
        }
    }
    pub fn read_all_registers(&mut self) -> Result<&str, &str> {
        match self.process_state {
            ProcessState::Stopped => {} // OK to proceed
            _ => return Err("Cannot read registers: process is not stopped"),
        }
        use nix::libc::{ptrace as libc_ptrace, PTRACE_GETFPREGS};

        let regs_libc = ptrace::getregs(self.pid);
        if let Err(ref e) = regs_libc {
            eprintln!("getregs errno: {}", e); // tells you ESRCH vs EPERM etc.
        }
        //.map_err(|_| "Couldnt read GPR registers")?;
        if let Ok(regs_libc) = regs_libc {
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
        } else {
            return Err("Couldnt Read GPR REGS!");
        }

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

    pub unsafe fn get_instruction_pointer_va(&self) -> Result<VirtAddr, &'static str> {
        let rip_val = self
            .proc_registers
            .get_register_val_by_id(RegisterId::Rip)
            .unwrap();

        match rip_val {
            RegisterValue::U64(val) => Ok(VirtAddr::with_addr(val)),
            _ => Err("Invalid Register value returned"),
        }
    }
    pub fn add_breakpoint_at(&mut self, addr: VirtAddr) -> Result<&mut BreakpointVA, String> {
        if self.stop_points.contains_address(&addr) {
            return Err(format!(
                "Breakpoint already created at address: {}",
                addr.addr()
            ));
        }
        let bp = BreakpointVA::create_for_process_at(self, addr);
        Ok(self.stop_points.push(bp))
    }
    unsafe fn set_instruction_pointer_va(&mut self, virt_addr: VirtAddr) {
        let register_value: RegisterValue = RegisterValue::U64(virt_addr.addr());
        self.write_to_user_by_register_id(RegisterId::Rip, register_value);
    }
    unsafe fn step_one_instruction(&mut self, args: Vec<&str>) {
        let rip = self.get_instruction_pointer_va();
        if let Err(e) = rip {
            eprintln!("Couldnt get RIP register");
            return;
        }
        let pid = self.pid();
        let mut flag_reenable = false;
        let rip = rip.unwrap();
        if self.stop_points.is_stoppoint_enabled_by_address(&rip) {
            let bp = self.stop_points.get_by_address_mut(&rip).unwrap();
            bp.disable(pid);
            flag_reenable = true;
        }
        // single step over the instruction we just restored
        if let Err(e) = ptrace::step(pid, None) {
            eprintln!("Failed to single step: {}", e);
            process::exit(1);
        }

        let wait_status = self.wait_on_signal();

        if let Err(e) = wait_status {
            eprintln!("waiting after stepping over single failed");
        }

        let pid = self.pid();
        if flag_reenable {
            let bp = self.stop_points.get_by_address_mut(&rip).unwrap();
            bp.enable(pid);
        }
    }
}
