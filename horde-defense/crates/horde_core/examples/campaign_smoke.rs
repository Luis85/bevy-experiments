#![allow(clippy::print_stdout, clippy::print_stderr)] // CLI example / test diagnostics
fn main() {
    match horde_core::sim::smoke() {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
