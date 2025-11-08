use std::{env, process};
use rdb::utils::process::attach;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 1 {
        eprintln!("give a process/binary path id to attach to");
        process::exit(1);
    }
    let pid:Result<i32, String> = attach(args);
}


