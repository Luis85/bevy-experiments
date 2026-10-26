//! Sim randomness: keyed, counter-based draws (spec §5.4, ADR-0003).
//!
//! A draw is a pure function of `(MatchSeed, tick, sim_id, stream_key(stream name), n)`; no
//! RNG state lives anywhere in the sim. [`rolls`] draws only from a [`Stream`], which only
//! [`SimBuilder::register_stream`](crate::SimBuilder::register_stream) hands out, so a draw
//! through [`rolls`] is from a registered stream, and two features cannot register one name.
//! The raw cursor (`fl_math::Rolls::new`, `fl_math::roll`) is a clippy-disallowed method in
//! every sim crate: only [`rolls`] itself and setup code that seeds its own draws (the scale
//! scenario's placement) use it, each behind a scoped `#[expect]`. `fl_math::stream_key` stays
//! public: the state hash keys names with it, and a key alone draws nothing. World-level draws
//! use `SimId(0)`. Draws for one entity do not depend on which other entities, streams
//! or features exist, so entity-local systems may iterate in any order.

use bevy_ecs::resource::Resource;
use fl_math::{Rolls, stream_key};
use fl_protocol::{SimId, Tick};
use serde::{Deserialize, Serialize};

use crate::hash::{SimHash, StateHasher};

/// The match seed, from the [`MatchHeader`](fl_protocol::MatchHeader). Hashed
/// `"core.match_seed"`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSeed(pub u64);

impl SimHash for MatchSeed {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u64(self.0);
    }
}

/// A registered random stream: the key [`rolls`] draws with, `fl_math::stream_key(name)`.
/// Only [`SimBuilder::register_stream`](crate::SimBuilder::register_stream) makes one. A
/// system gets it from its feature's `build`: captured by a closure, or passed in with
/// `IntoSystem::with_input` as an `InMut<Stream>` the system only copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stream {
    key: u64,
}

impl Stream {
    /// The stream named `name`; only the builder's registration calls this.
    pub(crate) fn named(name: &str) -> Stream {
        Stream {
            key: stream_key(name),
        }
    }
}

/// The draws of `stream` for `sim_id` at `tick`: a short-lived cursor, never stored.
#[expect(
    clippy::disallowed_methods,
    reason = "the one place a registered Stream becomes a cursor"
)]
pub fn rolls(seed: &MatchSeed, tick: Tick, sim_id: SimId, stream: Stream) -> Rolls {
    Rolls::new(seed.0, tick.0, sim_id.0, stream.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "compares rolls with the raw cursor it wraps"
    )]
    fn rolls_are_keyed_by_every_input() {
        let seed = MatchSeed(9);
        let wander = Stream::named("debug.wander");
        let draw = |seed: &MatchSeed, tick, id, stream| {
            rolls(seed, Tick(tick), SimId(id), stream).next_u64()
        };
        let base = draw(&seed, 3, 4, wander);
        assert_eq!(base, draw(&seed, 3, 4, wander));
        assert_ne!(base, draw(&MatchSeed(10), 3, 4, wander));
        assert_ne!(base, draw(&seed, 4, 4, wander));
        assert_ne!(base, draw(&seed, 3, 5, wander));
        assert_ne!(base, draw(&seed, 3, 4, Stream::named("setup.scale")));
        let key = stream_key("debug.wander");
        assert_eq!(base, Rolls::new(9, 3, 4, key).next_u64());
    }
}
