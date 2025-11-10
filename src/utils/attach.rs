use crate::rdb::process::Process;

pub fn attach(args: Vec<String>) -> Result<Process, String> {
    // -p target_pid
    if args.len() == 3 && args[1] == "-p" {
        // attach to an existing process
        Process::attach(&args[2])
    } else {
        //spin up a new process and attach to it
        let program_path = args[1].clone();
        Process::launch(&program_path)
    }
}