use std::path::Path;
use nix::sys::wait::waitpid;
use nix::unistd::Pid;
use rdb::rdb::process::Process;

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
