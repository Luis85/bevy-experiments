//! Explicit temporal data provenance and time-bounded geographic data.
//! Historical requests never fall through to current public APIs or synthetic
//! terrain. Political boundaries are source claims, separate from game factions.

mod pack;
mod politics;
mod time;

pub use pack::*;
pub use politics::*;
pub use time::{parse_date, valid_at};

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("historical data I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid historical data: {0}")]
    Invalid(String),
    #[error("requested time {requested} is outside [{from}, {until:?})")]
    OutsideTime {
        requested: i64,
        from: i64,
        until: Option<i64>,
    },
    #[error("historical tile is absent from this pack: {0:?}")]
    MissingChunk(terra_core::TileKey),
}
