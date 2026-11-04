//! Separate operator process: provider sessions never run inside simulation ticks.
#[path = "../ai_client/mod.rs"]
mod ai_client;

fn main() -> std::process::ExitCode {
    ai_client::main()
}
