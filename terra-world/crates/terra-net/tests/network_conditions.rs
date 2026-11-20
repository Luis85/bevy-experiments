#[path = "../examples/support/network_acceptance.rs"]
mod network_acceptance;

#[test]
fn conditioned_tcp_slow_reader_catches_up_without_starving_peer_or_losing_private_state() {
    let report = network_acceptance::run(false).unwrap();
    assert_eq!(report["status"], "passed");
}

#[cfg(feature = "tls")]
#[test]
fn conditioned_tls_slow_reader_catches_up_then_resumes_exactly_after_restart() {
    let report = network_acceptance::run(true).unwrap();
    assert_eq!(report["status"], "passed");
}
