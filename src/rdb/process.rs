use std::ffi::CString;
use std::process;
use nix::errno::Errno;
use nix::sys::ptrace;
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{execvp, fork, ForkResult, Pid};


pub struct Process {
    pid: Pid,
    terminate_on_end: bool,
    pub process_state: ProcessState
}

#[derive(Copy, Clone)]
pub enum ProcessState {
    Stopped,
    Running,
    Exited,
    Terminated
}

impl Drop for Process{
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
            if let Err(e) = kill(self.pid, Signal::SIGKILL){
                eprintln!("Failed to kill process {}: {}", self.pid, e);
            }else{
                waitpid(self.pid, None);
            }
        }
    }
}

impl Process {
    fn new(pid: Pid, terminate_on_end: bool, process_state: ProcessState) -> Self{
        Self {
            pid,
            terminate_on_end,
            process_state
        }
    }
    pub fn pid(&self) ->Pid{
        self.pid
    }
    pub fn attach(pid_arg: &str) -> Result<Process, String> {
        let pid = pid_arg
            .parse::<i32>()
            .map_err(|_|"Invalid PID: not a valid number")?;

        if pid <= 0 {
            return Err("Invalid PID: must be positive".to_string())
        }

        ptrace::attach(Pid::from_raw(pid))
            .map_err(|e| format!("Failed to attach: {}", e))?;

        let process_state = ProcessState::Running;

        let terminate_on_end = false;

        let process = Process::new(Pid::from_raw(pid), terminate_on_end, process_state);

        Ok(process)
    }
    pub fn launch(program_path: &str) -> Result<Process, String>{
        unsafe {
            let fork_res = fork();
            match fork_res {
                Ok(ForkResult::Parent {child}) => {
                    let pid = child.as_raw();

                    let process_state = ProcessState::Running;
                    let terminate_on_end = true;
                    let process = Process::new(Pid::from_raw(pid), terminate_on_end, process_state);

                    Ok(process)
                }
                Ok(ForkResult::Child) => {
                    let traceme_res = ptrace::traceme();
                    if let Err(e) = traceme_res {
                        eprintln!("Tracing child process failed: {}", e);
                        process::exit(1);
                    }
                    let program_path_c = CString::new(program_path)
                        .expect("Cstring conversion failed");
                    let exec_args = vec![program_path_c.clone()];
                    let exec_res = execvp(&program_path_c, &exec_args);
                    if let Err(e) = exec_res {
                        eprintln!("Exec failed: {}", e);
                        process::exit(1);
                    }
                    unreachable!();
                }
                Err(e) => {
                    Err(format!("Fork failed: {}", e))
                }
            }
        }
    }
    pub fn dispatch_command(&mut self, command: String) {
        let args: Vec<&str> = command.split_whitespace().collect();
        let command = args[0];
        if "continue".starts_with(command) {
            self.resume();
            if let Err(e) = self.wait_on_signal(){
                process::exit(1);
            } // breakpoint// process stops again
        } else {
            eprintln!("unknown command: {}", command)
        }
    }
    fn resume(&mut self){
        if let Err(e) = ptrace::cont(self.pid(), None){
            eprintln!("Couldn't Continue: {}", e);
            process::exit(1);
        }
        self.process_state = ProcessState::Running;
    }
    fn wait_on_signal(&mut self) -> Result<WaitStatus, Errno>{
        let wait_res = waitpid(self.pid, None);
        match wait_res {
            Ok(status) => {
                self.process_state = ProcessState::Stopped;
                Ok(status)
            }
            Err(e) => {
                eprintln!("waitpid failed: {}", e);
                self.process_state=ProcessState::Terminated;
                Err(e)
            }
        }
    }
}