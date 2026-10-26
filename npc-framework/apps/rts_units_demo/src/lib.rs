//! `rts_units_demo`: the portability proof (hardening spec goal 5). A
//! self-contained RTS host app that depends only on the `npc_framework`
//! facade (+ `bevy`): its own unit types, map, host physics in
//! `FixedUpdate`, a 10 Hz sim, selection, control groups and formations.
//! `arch_fitness` pins that it never reaches a `content/*` crate.

pub mod host;
pub mod sim;
pub mod view;
