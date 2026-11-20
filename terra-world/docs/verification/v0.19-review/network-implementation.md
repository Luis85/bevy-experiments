# 0.19 network privacy implementation

Implements NET-01 from the approved increment plan. `ClientMessage` and `ServerMessage` keep Clone/Serialize/Deserialize and their exact tagged protocol shape, but their Debug implementations now explicitly redact Hello/Welcome resume capabilities. Hello retains Some/None presence; Welcome retains useful nonsecret handshake fields. All other variants retain their named fields. The exhaustive matches require review when a future variant is added.

The regression `protocol::tests::capability_debug_redaction_preserves_private_wire_handshakes` covers normal, alternate and nested formatting, no token/prefix disclosure, guest None, useful actor identification, exact expected Hello/Welcome JSON and both deserialization round trips. Protocol remains 10 and ALPN remains `terra-world/10`; no save, regional or asset change occurs.

`rustfmt --edition 2024 crates/terra-net/src/protocol.rs` completed successfully. Cargo execution is deliberately owned by the integration agent; no Rust test run is claimed here. Intended focused command: `cargo test -p terra-net --lib capability_debug_redaction_preserves_private_wire_handshakes --locked --offline`; existing protocol and network/default-feature variants remain integration acceptance.

NET-02 remains separately documented: consolidating SDK admission would alter a different boundary and is not silently included. The redaction protects this typed credential field in Debug. Explicit JSON serialization intentionally carries credentials and must still be kept out of logs; arbitrary user-authored strings and gameplay state remain outside this field-specific protection.
