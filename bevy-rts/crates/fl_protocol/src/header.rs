//! [`MatchHeader`]: what a match, replay or save was made with (spec §5.5, ADR-0007).

use core::fmt;
use serde::{Deserialize, Serialize};

use crate::tables::StringTables;

/// Version of the replay, save and wire format that carries a [`MatchHeader`].
pub const FORMAT_VERSION: u32 = 1;

/// Everything two peers, or a match and its replay or save, must agree on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchHeader {
    pub format_version: u32,
    pub build_id: String,
    pub sim_version: u32,
    /// Sim features as `(name, version)`, in load order: the topological order of their
    /// dependencies (core first, ties by id), not sorted by id.
    pub features: Vec<(String, u32)>,
    /// Mods as `(id, version)`, in load order.
    pub mods: Vec<(String, String)>,
    /// Canonical hash of the rules-relevant content (ADR-0007).
    pub rules_hash: u64,
    /// Whether `debug.*` commands are accepted.
    pub cheats_allowed: bool,
    pub seed: u64,
    pub string_tables: StringTables,
}

/// The first [`MatchHeader`] field that makes two headers incompatible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HeaderMismatch {
    FormatVersion,
    BuildId,
    SimVersion,
    Features,
    Mods,
    RulesHash,
    CheatsAllowed,
    Seed,
    StringTables,
}

impl fmt::Display for HeaderMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self {
            HeaderMismatch::FormatVersion => "format version",
            HeaderMismatch::BuildId => "build id",
            HeaderMismatch::SimVersion => "sim version",
            HeaderMismatch::Features => "features",
            HeaderMismatch::Mods => "mods",
            HeaderMismatch::RulesHash => "rules hash",
            HeaderMismatch::CheatsAllowed => "cheats allowed",
            HeaderMismatch::Seed => "seed",
            HeaderMismatch::StringTables => "string tables",
        };
        write!(f, "match header mismatch: {what} differs")
    }
}

impl std::error::Error for HeaderMismatch {}

/// `Err(mismatch)` unless `same`.
fn require(same: bool, mismatch: HeaderMismatch) -> Result<(), HeaderMismatch> {
    if same { Ok(()) } else { Err(mismatch) }
}

impl MatchHeader {
    /// Can this build play back a replay recorded with `recorded`? Format version, features,
    /// mods, sim version and rules hash must be equal. A different build id is allowed and
    /// returned as a warning. The seed, cheats flag and string tables come from the replay.
    pub fn check_replay(&self, recorded: &MatchHeader) -> Result<Vec<String>, HeaderMismatch> {
        require(
            self.format_version == recorded.format_version,
            HeaderMismatch::FormatVersion,
        )?;
        require(self.features == recorded.features, HeaderMismatch::Features)?;
        require(self.mods == recorded.mods, HeaderMismatch::Mods)?;
        require(
            self.sim_version == recorded.sim_version,
            HeaderMismatch::SimVersion,
        )?;
        require(
            self.rules_hash == recorded.rules_hash,
            HeaderMismatch::RulesHash,
        )?;
        let mut warnings = Vec::new();
        if self.build_id != recorded.build_id {
            warnings.push(format!(
                "replay recorded with build {:?}, playing back with build {:?}",
                recorded.build_id, self.build_id
            ));
        }
        Ok(warnings)
    }

    /// Can this peer play a lockstep match with `peer`? Every field must be equal, build id
    /// included.
    pub fn check_handshake(&self, peer: &MatchHeader) -> Result<(), HeaderMismatch> {
        require(
            self.format_version == peer.format_version,
            HeaderMismatch::FormatVersion,
        )?;
        require(self.build_id == peer.build_id, HeaderMismatch::BuildId)?;
        require(
            self.sim_version == peer.sim_version,
            HeaderMismatch::SimVersion,
        )?;
        require(self.features == peer.features, HeaderMismatch::Features)?;
        require(self.mods == peer.mods, HeaderMismatch::Mods)?;
        require(
            self.rules_hash == peer.rules_hash,
            HeaderMismatch::RulesHash,
        )?;
        require(
            self.cheats_allowed == peer.cheats_allowed,
            HeaderMismatch::CheatsAllowed,
        )?;
        require(self.seed == peer.seed, HeaderMismatch::Seed)?;
        require(
            self.string_tables == peer.string_tables,
            HeaderMismatch::StringTables,
        )
    }

    /// Can this build load a save made with `saved`? The save's format version must not be
    /// newer than ours; the features must be the same ids, each saved at a version no newer
    /// than ours (an older one is fine: the per-component and per-resource codec versions and
    /// their migrations decide what decodes); the mods must be equal.
    pub fn check_save(&self, saved: &MatchHeader) -> Result<(), HeaderMismatch> {
        require(
            saved.format_version <= self.format_version,
            HeaderMismatch::FormatVersion,
        )?;
        require(
            features_not_newer(&saved.features, &self.features),
            HeaderMismatch::Features,
        )?;
        require(self.mods == saved.mods, HeaderMismatch::Mods)
    }
}

/// Whether `saved` and `ours` list the same feature ids, and no feature of `saved` is newer
/// than in `ours`. The order does not matter: it follows from the features' dependencies.
fn features_not_newer(saved: &[(String, u32)], ours: &[(String, u32)]) -> bool {
    fn sorted(features: &[(String, u32)]) -> Vec<(&str, u32)> {
        let mut features: Vec<(&str, u32)> = features
            .iter()
            .map(|(id, version)| (id.as_str(), *version))
            .collect();
        features.sort_unstable();
        features
    }
    let (saved, ours) = (sorted(saved), sorted(ours));
    saved.len() == ours.len()
        && saved
            .iter()
            .zip(&ours)
            .all(|((saved_id, saved_v), (id, v))| saved_id == id && saved_v <= v)
}

/// Who controls a player slot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Controller {
    Human,
    Ai(String),
    None,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SIM_TICK_HZ;

    #[test]
    fn controller_ron_names() {
        assert_eq!(ron::to_string(&Controller::Human).unwrap(), "Human");
        assert_eq!(SIM_TICK_HZ, 20);
    }

    #[test]
    fn header_mismatch_display() {
        assert_eq!(
            HeaderMismatch::RulesHash.to_string(),
            "match header mismatch: rules hash differs"
        );
    }
}
