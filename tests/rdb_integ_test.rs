use std::ffi::CString;
use std::path::Path;
use std::process;
use std::time::Duration;
use nix::fcntl::{fcntl, FcntlArg, FdFlag};
use nix::libc::pid_t;
use nix::sys::ptrace;
use nix::sys::wait::waitpid;
use nix::unistd::{close, execvp, fork, pipe, read, write, ForkResult, Pid};
use rdb::rdb::process::{Process, ProcessState};

#[test]
fn test_process_launch_success(){
    let proc = Process::launch("yes")
        .expect("Failed to launch process");

    assert!(process_running(proc.pid()));
    drop(proc);
}

fn process_running(pid: Pid) -> bool {
    // proc is a special directory on linux virtual Filesystem
    // /proc/123
    // /proc/123/stat
    // Each running process has a directory named after its PID
    Path::new(&format!("/proc/{}", pid.as_raw())).exists()
}

// #[test]
// fn test_process_launch_nonexistent_process(){
//     let proc = Process::launch("/random/non/existent/path/hopefully")
//         .expect("Failed to launch process");
//     let wait_res = waitpid(proc.pid(), None);
//     match wait_res{
//         Ok(_) => {
//             println!("-----------------------------{}------------------------------", process_running(proc.pid()));
//             assert!(!process_running(proc.pid()));
//         }
//         Err(_) => {
//             assert!(false);
//         }
//     }
//     drop(proc);
// }

#[test]
fn test_process_launch_nonexistent_process(){
    let proc = Process::launch("/random/non/existent/path/hopefully");
    if let Err(_) = proc {
        assert!(true)
    }else{
        assert!(false)
    }
}



fn launch_test_process(program_path: &str) -> Result<Process, String> {
    let (read_fd, write_fd) = pipe().map_err(|e|format!("pipe failed: {}", e))?;

    fcntl(&read_fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).ok();

    fcntl(&write_fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).ok();

    unsafe {
        let fork_res = fork();
        match fork_res {
            Ok(ForkResult::Parent {child}) => {
                close(write_fd).ok(); //  we only want to read from the parent

                let mut buffer:[u8;256] = [0;256];

                let bytes_read = read(&read_fd, &mut buffer).unwrap_or(0);

                close(read_fd).ok();

                let pid = child.as_raw();
                let process_state = ProcessState::Running;
                let terminate_on_end = true;
                let process = Process::new(Pid::from_raw(pid), terminate_on_end, process_state);

                if bytes_read > 0 {
                    drop(process);
                    let msg = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                    return Err(format!("Child process failed with error: {}", msg));
                }

                Ok(process)
            }
            Ok(ForkResult::Child) => {
                close(read_fd).ok(); // we only want to write from the child

                let program_path_c = CString::new(program_path)
                    .expect("Cstring conversion failed");
                let exec_args = vec![program_path_c.clone()];
                let exec_res = execvp(&program_path_c, &exec_args);

                // if the exec in the above line works fine then we never write something to the pipe
                // nor do we ever close it

                if let Err(e) = exec_res {
                    let _ = write(&write_fd, format!("Tracing child process failed: {}", e).as_bytes());
                    eprintln!("Exec failed: {}", e);
                    close(write_fd).ok();
                    return Err(format!("Exec Failed!: {}", e))
                }
                unreachable!();
            }
            Err(e) => {
                Err(format!("Fork failed: {}", e))
            }
        }
    }
}

#[test]
fn test_process_attach_success(){
    let launch_res = launch_test_process("yes");

    match launch_res{
        Ok(proc) => {
            let pid_arg = proc.pid().as_raw().to_string();
            let attach_res = Process::attach(&pid_arg);
            if let Err(_) = attach_res {
                assert!(false, "attach_failed!")
            }
            std::thread::sleep(Duration::from_millis(50));
            let process_state: Result<char, String> = get_process_state(proc.pid().as_raw() as u32);
            match process_state {
                Ok(c) => {
                    assert_eq!(c,'t')
                }
                Err(s) => {
                    assert!(false, "{}", s)
                }
            }
        }
        Err(s) => {
            assert!(false, "{}", s);
        }
    }
}

fn get_process_state(pid: u32) -> Result<char, String> {
    // extract the process state from /proc/{pid}/stat
    let path = format!("/proc/{}/stat", pid);
    let data = std::fs::read_to_string(path.clone());
    let data = if let Ok(d) = data {
        d
    }else{
        return Err(format!("Failed to read data from {}", path));
    };

    let paren_index = data.rfind(')');

    match paren_index {
        None => {
            Err("Data in invalid format".to_string())
        }
        Some(i) => {
            let state_index = i+2;
            let state = data.chars().nth(state_index);
            match state {
                None => {
                    Err("Data in invalid format".to_string())
                }
                Some(c) => {
                    Ok(c)
                }
            }
        }
    }
}

#[test]
fn test_process_attach_pid_0_fails(){
    match Process::attach("0"){
        Ok(_) => {
            assert!(false, "attached to process with pid 0")
        }
        Err(_) => {
            assert!(true)
        }
    }
}