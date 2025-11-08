use std::ffi::CString;

use std::process;
use nix::sys::ptrace;
use nix::unistd::{execvp, fork, ForkResult, Pid};

pub fn attach(args: Vec<String>) -> Result<i32, String> {
    // -p target_pid
    if args.len() == 3 && args[1] == "-p" {
        // attach to an existing process
        let pid = args[2]
            .parse::<i32>()
            .map_err(|_|"Invalid PID: not a valid number")?;

        if pid <= 0 {
            return Err("Invalid PID: must be positive".to_string())
        }

        ptrace::attach(Pid::from_raw(pid))
            .map_err(|e| format!("Failed to attach: {}", e))?;

        Ok(pid)
    } else {
        //spin up a new process and attach to it
        let program_path = args[1].clone();

        unsafe {
            let fork_res = fork();
            match fork_res {
                Ok(ForkResult::Parent {child}) => {
                    let pid = child.as_raw();
                    Ok(pid)
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
    // "path.exe" -> spin up a new process
}