//! Reproduce bounded delayed/fragmented/slow-reader TCP and TLS acceptance.
#[path = "support/network_acceptance.rs"]
mod network_acceptance;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let encrypted = match std::env::args().nth(1).as_deref() {
        None | Some("tls") => true,
        Some("tcp") => false,
        Some(_) => return Err("usage: network_conditions [tcp|tls]".into()),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&network_acceptance::run(encrypted)?)?
    );
    Ok(())
}
