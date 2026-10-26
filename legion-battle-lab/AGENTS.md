# Legion Battle Lab assistant guide

Read `README.md`, `ARCHITECTURE.md`, `PRODUCT.md` and `TEST_RESULTS.md`.
The Rust-only `battle_core` owns battle rules and stable per-unit state;
`legion` owns UI, the worker bridge and rendering. Preserve bounded worker queues,
generation checks, stable hit application and camera-independent activity LOD.
Do not add one Bevy entity per crowd unit without an explicit architectural change.

Run Cargo here:

```sh
cargo test --locked -p battle_core
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo run --locked --release -p legion
```

Use `cargo fmt --all -- --check` before completion. Keep scenarios and asset
licenses. Existing `validation/` files are historical evidence; only claim new
measurements after running the documented native checks. A million stored/rendered
units is a capacity goal, not a promised real-time frame or tick rate. Use release
builds for performance evidence. Put local saves in ignored `user-data/`.
