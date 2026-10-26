//! The save format (spec §5.8, ADR-0006) and the contexts codecs encode and decode with.

use std::collections::BTreeMap;

use fl_protocol::{MatchHeader, PlayerCommand, SimId, StringTables, Tick};
use serde::{Deserialize, Serialize};

/// The version of the [`SaveFile`] layout. Component and resource versions are separate: each
/// [`Codec`](crate::Codec) has its own.
pub const SAVE_FORMAT_VERSION: u32 = 1;

/// One saved component or resource: `(name, version, bytes)`, the bytes being its
/// [`Codec`](crate::Codec)'s encoding at that version.
pub type SavedValue = (String, u32, Vec<u8>);

/// A save, as postcard bytes. `format_version` comes first, so a loader reads it before the
/// rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveFile {
    pub format_version: u32,
    /// The match it was saved from, with every string table.
    pub header: MatchHeader,
    pub sim: SimSave,
}

/// The sim state of a [`SaveFile`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimSave {
    /// The next tick to run ([`SimClock`](crate::SimClock)).
    pub tick: Tick,
    /// The [`SimIdAllocator`](crate::SimIdAllocator)'s state: the last id it issued, 0 before
    /// the first. The next id is one above it.
    pub sim_id_next: u32,
    /// Commands stamped but not yet delivered, sorted by `(exec_tick, player, seq)`.
    pub pending_commands: Vec<PlayerCommand>,
    /// `(name, version, bytes)` of every `Hashed` resource the world has, sorted by name.
    pub resources: Vec<SavedValue>,
    /// Every entity: its id and the `(name, version, bytes)` of every `Hashed` component it
    /// has, sorted by `SimId`, then component name.
    pub entities: Vec<(SimId, Vec<SavedValue>)>,
}

/// What a [`Codec`](crate::Codec) encodes with.
pub struct SaveCtx<'a> {
    tables: &'a StringTables,
}

impl<'a> SaveCtx<'a> {
    pub(crate) fn new(tables: &'a StringTables) -> Self {
        SaveCtx { tables }
    }

    /// The string tables of the match being saved (they go into the save's header).
    pub fn tables(&self) -> &StringTables {
        self.tables
    }
}

/// What a [`Codec`](crate::Codec) decodes with: the string tables of the save and of the
/// sim it loads into.
pub struct LoadCtx<'a> {
    saved: &'a StringTables,
    current: &'a StringTables,
    /// Per table in both: saved dense id → current dense id, `None` when the name is gone.
    remaps: BTreeMap<&'a str, Vec<Option<u32>>>,
}

impl<'a> LoadCtx<'a> {
    pub(crate) fn new(saved: &'a StringTables, current: &'a StringTables) -> Self {
        let remaps = saved
            .tables
            .keys()
            .filter(|table| current.get(table).is_some())
            .map(|table| (table.as_str(), saved.remap(table, current)))
            .collect();
        LoadCtx {
            saved,
            current,
            remaps,
        }
    }

    /// The string tables of the save.
    pub fn saved_tables(&self) -> &StringTables {
        self.saved
    }

    /// The string tables of the sim being loaded into.
    pub fn current_tables(&self) -> &StringTables {
        self.current
    }

    /// The current dense id of the name that had dense id `old` in the save's `table`.
    ///
    /// # Errors
    /// The save or this sim has no table `table`, the save's table has no id `old`, or the
    /// name no longer exists.
    pub fn remap(&self, table: &str, old: u32) -> Result<u32, String> {
        let Some(remap) = self.remaps.get(table) else {
            return Err(if self.saved.get(table).is_none() {
                format!("the save has no string table '{table}'")
            } else {
                format!("this sim has no string table '{table}'")
            });
        };
        let slot = usize::try_from(old)
            .ok()
            .and_then(|i| remap.get(i))
            .ok_or_else(|| {
                format!(
                    "id {old} is not in the save's string table '{table}' ({} names)",
                    remap.len()
                )
            })?;
        slot.ok_or_else(|| {
            let name = self
                .saved
                .get(table)
                .and_then(|names| names.get(usize::try_from(old).ok()?))
                .map_or("?", String::as_str);
            format!("'{name}' (id {old} in the save's string table '{table}') no longer exists")
        })
    }
}
