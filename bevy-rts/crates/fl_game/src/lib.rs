//! fl_game: the testbed. The only crate that links the sim (`fl_sim`) and the client
//! (`fl_client`): [`GameBuilder`] loads the content, starts the sim on its own thread and
//! builds the client `App` around it (spec §5.1, ADR-0001, ADR-0002). It also hot-reloads the
//! content ([`reload_content`], ADR-0012) and captures a replay of the match
//! ([`ReplayCapture`]).

mod builder;
pub mod reload;
pub mod replay_capture;

pub use builder::{GameBuilder, StartMode, check_client_features};
pub use reload::{
    ContentSource, ContentWatcher, MatchTerrain, NetDriven, ReloadOutcome, reload_content,
};
pub use replay_capture::ReplayCapture;
