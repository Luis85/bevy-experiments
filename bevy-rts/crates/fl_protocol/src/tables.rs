//! String tables: the frozen name lists behind every dense id (ADR-0004).
//!
//! A [`StringTables`] travels in the [`MatchHeader`](crate::MatchHeader), so a replay or save
//! can map its recorded dense ids onto the current build's ids by name.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Commands, by [`CommandPayload::NAME`](crate::CommandPayload::NAME).
pub const COMMANDS: &str = "commands";
/// Def component names.
pub const DEF_COMPONENTS: &str = "def_components";
/// Object kind names.
pub const OBJECT_KINDS: &str = "object_kinds";
/// Content kind names.
pub const CONTENT_KINDS: &str = "content_kinds";
/// Rules section names.
pub const RULES_SECTIONS: &str = "rules_sections";
/// Sim component names.
pub const SIM_COMPONENTS: &str = "sim_components";
/// Sim resource names.
pub const SIM_RESOURCES: &str = "sim_resources";
/// RNG stream names.
pub const RNG_STREAMS: &str = "rng_streams";
/// Journal record kinds.
pub const JOURNAL_RECORDS: &str = "journal_records";
/// View channel names.
pub const VIEW_CHANNELS: &str = "view_channels";
/// Unit types. Other content kinds use `"content.<ext>"`.
pub const UNITS: &str = "content.unit";

/// Named tables of names; each table lists its names in dense-id order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringTables {
    /// Table name → names in dense-id order.
    pub tables: BTreeMap<String, Vec<String>>,
}

impl StringTables {
    /// Set `table` to `names`, replacing any previous list.
    pub fn insert(&mut self, table: &str, names: Vec<String>) {
        self.tables.insert(table.to_owned(), names);
    }

    /// The names of `table` in dense-id order.
    #[must_use]
    pub fn get(&self, table: &str) -> Option<&[String]> {
        self.tables.get(table).map(Vec::as_slice)
    }

    /// The dense id of `name` in `table`.
    #[must_use]
    pub fn id_of(&self, table: &str, name: &str) -> Option<u32> {
        let index = self.get(table)?.iter().position(|n| n == name)?;
        u32::try_from(index).ok()
    }

    /// Map every id of `table` in `self` to the id with the same name in `current`.
    /// The index is the id in `self`; the value is the id in `current`, or `None` when the
    /// name is gone. Empty when `self` has no such table.
    #[must_use]
    pub fn remap(&self, table: &str, current: &StringTables) -> Vec<Option<u32>> {
        // First occurrence wins, matching `id_of`.
        let mut ids = BTreeMap::<&str, u32>::new();
        for (id, name) in current.get(table).unwrap_or_default().iter().enumerate() {
            if let Ok(id) = u32::try_from(id) {
                ids.entry(name.as_str()).or_insert(id);
            }
        }
        self.get(table)
            .unwrap_or_default()
            .iter()
            .map(|name| ids.get(name.as_str()).copied())
            .collect()
    }
}
