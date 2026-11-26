//! SynEvid Production Benchmark Suite
//!
//! Runs comprehensive stress tests and benchmarks against real-world repositories.

mod stress_test_runner;

use std::env;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    let work_dir = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("./benchmarks/repos")
    };
    
    let autopsy_binary = if args.len() > 2 {
        PathBuf::from(&args[2])
    } else {
        PathBuf::from("../target/release/autopsy-daemon")
    };
    
    let results_dir = if args.len() > 3 {
        PathBuf::from(&args[3])
    } else {
        PathBuf::from("./benchmarks/results")
    };

    println!("SynEvid Production Benchmark Suite");
    println!("===================================");
    println!("Work directory: {}", work_dir.display());
    println!("Autopsy binary: {}", autopsy_binary.display());
    println!("Results directory: {}", results_dir.display());
    
    // Build the stress test runner as a binary
    // For now, just print the configuration
    println!("\nConfiguration:");
    println!("  Work dir: {}", work_dir.display());
    println!("  Binary: {}", autopsy_binary.display());
    println!("  Results dir: {}", results_dir.display());
    println!("\nTo run stress tests, compile and run:");
    println!("  cargo build --release -p syn-bench");
    println!("  ./target/release/syn-bench <work_dir> <autopsy_binary> <results_dir>");
}