use std::{env, process};
use nix::sys::wait::{waitpid, WaitStatus};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use rdb::rdb::process::{Process, ProcessState};
use rdb::utils::attach::attach;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 1 {
        eprintln!("give a process/binary path id to attach to");
        process::exit(1);
    }
    let process:Result<Process, String> = attach(args);
    let mut process = match process {
        Ok(p) => {p}
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };
    let wait_res = waitpid(process.pid(), None);
    match wait_res {
        Ok(WaitStatus::Stopped(child_pid, signal)) => {
            println!("Process {} stopped by signal {:?}", child_pid, signal);
            process.process_state = ProcessState::Stopped;
            debug(process);
        }
        Ok(status) => {
            eprintln!("Unexpected Status: {:?}", status);
            process.process_state = ProcessState::Exited;
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Wait Pid failed: {}", e);
            process.process_state = ProcessState::Terminated;
            process::exit(1);
        }
    }
}

fn debug(mut process: Process) {
    let mut rl = DefaultEditor::new().unwrap();
    if let Err(e) = rl.load_history(".history"){
        println!("No previous history.");
    }
    loop {
        let readline = rl.readline("rdb>> ");
        match readline {
            Ok(line) => {
                if line != "" {
                    let _ = rl.add_history_entry(line.as_str());
                    process.dispatch_command(line);
                    // we want to handle command formats similar to GDB
                    // so to continue a program a user can say just continue, cont or c
                    // to set a breakpoint on an address >> break set 0xabcdabcd
                }
            },
            Err(ReadlineError::Interrupted) => {
                // CTRL + c
                println!("^C");
                continue;
            },
            Err(ReadlineError::Eof) => {
                // CTRL + d
                println!("Exiting Debugger - CTRL + D");
                drop(process);
                break;
            },
            Err(e) => {
                eprintln!("Error: {:?}", e);
                break;
            }
        }
    }
    let _ = rl.save_history(".history");
}


