//! Render still frames with the CPU evaluator.
//!
//! Usage: stills <seed> <count> <size> <output-directory> [seconds-between-frames]

use psychedelic::stills::{Backend, StillsRequest, render};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let request = StillsRequest::from_args(&args, Backend::Cpu).unwrap_or_else(fail);
    for path in render(&request).unwrap_or_else(fail) {
        println!("{}", path.display());
    }
}

fn fail<T>(message: String) -> T {
    eprintln!("{message}");
    std::process::exit(1);
}
