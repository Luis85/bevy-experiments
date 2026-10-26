//! Sim observability (spec §15 B, ADR-0007): the event [`Journal`] and the `fl_sim::phase`
//! marker events.
//!
//! Both are observe-only: sim logic never reads them, and they are never hashed or saved.
//!
//! - **Journal.** Systems record [`JournalRecord`]s, each of a kind registered with
//!   [`SimBuilder::register_journal_record`](crate::SimBuilder::register_journal_record) (a
//!   stable-string registry exported as the `"journal_records"` string table).
//!   [`step_tick`](crate::step_tick) drains the tick's records into
//!   [`TickOutput::journal`](fl_view::TickOutput::journal), sorted by `(phase, kind, sim_id,
//!   emission order)`, so the journal does not depend on the order of unordered systems. It is
//!   on in debug builds unless [`RunConfig::journal`](crate::RunConfig::journal) says
//!   otherwise.
//! - **Phase markers.** With [`RunConfig::phase_markers`](crate::RunConfig::phase_markers),
//!   a marker system before and after each [`Phase`] emits
//!   `tracing::trace!(target: "fl_sim::phase", phase = "<Phase>", edge = "start" | "end")`.
//!   The sim never reads a clock: `fl_observe` timestamps the events.

use bevy_app::App;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use fl_protocol::{PlayerId, SimId, Tick};
use fl_view::JournalEntry;
use serde::Serialize;

use crate::registry::StableRegistry;
use crate::schedule::{Phase, SimTick};

/// The tracing target of the phase marker events.
pub const PHASE_TARGET: &str = "fl_sim::phase";

/// The phases in run order, with the names the markers carry.
const PHASES: [(Phase, &str); 8] = [
    (Phase::Input, "Input"),
    (Phase::Think, "Think"),
    (Phase::Act, "Act"),
    (Phase::Resolve, "Resolve"),
    (Phase::Economy, "Economy"),
    (Phase::Perceive, "Perceive"),
    (Phase::Lifecycle, "Lifecycle"),
    (Phase::Publish, "Publish"),
];

/// A kind of journal record: its data, serialized to JSON in the entry. Register each kind
/// with [`SimBuilder::register_journal_record`](crate::SimBuilder::register_journal_record).
///
/// **One system per group.** Within a tick, only one system may record a given
/// `(phase, kind, sim_id)` group. The per-tick sort orders groups, and keeps each group's
/// records in emission order; two unordered systems writing records of the same group would
/// still interleave in schedule order, so the journal would depend on the system order.
pub trait JournalRecord: Serialize + std::fmt::Debug + Send + Sync + 'static {
    /// The stable, namespaced kind, such as `"core.command_applied"`.
    const KIND: &'static str;
}

/// The journal records of the running tick. Transient `"core.journal"`; unordered systems may
/// record into it (its ambiguities are allowed), because [`step_tick`](crate::step_tick)
/// sorts each tick's records by `(phase, kind, sim_id)` and ranks each record within its
/// group by emission order.
///
/// That makes the journal independent of the system order only if **one system records each
/// `(phase, kind, sim_id)` group** in a tick (see [`JournalRecord`]): unordered systems
/// writing the same group are not made deterministic by the sort.
#[derive(Resource)]
pub struct Journal {
    enabled: bool,
    /// The registered kinds, frozen.
    kinds: StableRegistry<()>,
    entries: Vec<JournalEntry>,
}

impl Journal {
    /// An empty, disabled journal over the frozen `kinds`.
    pub(crate) fn new(kinds: StableRegistry<()>) -> Self {
        Journal {
            enabled: false,
            kinds,
            entries: Vec::new(),
        }
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether records are kept; a system may skip building a record when not.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Record `rec`, emitted in `phase` of `tick` about entity `sim_id` on behalf of `player`.
    /// Does nothing while the journal is disabled.
    ///
    /// # Panics
    /// In debug builds, if `R::KIND` is not registered.
    pub fn record<R: JournalRecord>(
        &mut self,
        tick: Tick,
        phase: Phase,
        sim_id: Option<SimId>,
        player: Option<PlayerId>,
        rec: &R,
    ) {
        if !self.enabled {
            return;
        }
        debug_assert!(
            self.kinds.id(R::KIND).is_some(),
            "journal record kind '{}' is not registered (SimBuilder::register_journal_record)",
            R::KIND
        );
        let data = serde_json::to_value(rec)
            .unwrap_or_else(|err| panic!("journal record '{}' serializes: {err}", R::KIND));
        let emitted = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
        self.entries.push(JournalEntry {
            tick,
            phase: phase_index(phase),
            kind: R::KIND.to_owned(),
            sim_id,
            player,
            emitted,
            data,
        });
    }

    /// The number of records not yet taken.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Take every record so far, sorted by `(tick, phase, kind, sim_id, emitted)`. Then
    /// `emitted` becomes the record's emission order among the taken records with the same
    /// tick, phase, kind and sim id, so the result does not depend on the order in which
    /// unordered systems ran (as long as one system emits each such group).
    pub(crate) fn take_sorted(&mut self) -> Vec<JournalEntry> {
        let mut entries = std::mem::take(&mut self.entries);
        entries.sort_by(|a, b| {
            (a.tick, a.phase, &a.kind, a.sim_id, a.emitted)
                .cmp(&(b.tick, b.phase, &b.kind, b.sim_id, b.emitted))
        });
        let mut rank = 0;
        for i in 0..entries.len() {
            rank = match i.checked_sub(1).map(|prev| &entries[prev]) {
                Some(prev) if same_group(prev, &entries[i]) => rank + 1,
                _ => 0,
            };
            entries[i].emitted = rank;
        }
        entries
    }
}

/// Whether `a` and `b` have the same tick, phase, kind and sim id.
fn same_group(a: &JournalEntry, b: &JournalEntry) -> bool {
    a.tick == b.tick && a.phase == b.phase && a.kind == b.kind && a.sim_id == b.sim_id
}

/// `phase`'s position in run order, as [`JournalEntry::phase`] carries it.
fn phase_index(phase: Phase) -> u8 {
    PHASES
        .iter()
        .position(|&(p, _)| p == phase)
        .and_then(|i| u8::try_from(i).ok())
        .expect("every phase is listed")
}

/// Where a phase's start (`true`) or end (`false`) marker runs.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PhaseMarker(Phase, bool);

/// The marker system for `phase`'s `edge`.
fn marker(phase: &'static str, edge: &'static str) -> impl Fn() + Send + Sync + 'static {
    move || tracing::trace!(target: PHASE_TARGET, phase, edge)
}

/// Add the phase markers to [`SimTick`]: each phase's start marker runs after the previous
/// phase's end marker and before the phase; its end marker after the phase (and its sync
/// point) and before the next phase. The markers access nothing, so they conflict with no
/// system.
pub(crate) fn add_phase_markers(app: &mut App) {
    for (i, &(phase, name)) in PHASES.iter().enumerate() {
        let start = PhaseMarker(phase, true);
        let end = PhaseMarker(phase, false);
        app.configure_sets(SimTick, (start.before(phase), end.after(phase)));
        if let Some(&(next, _)) = PHASES.get(i + 1) {
            app.configure_sets(SimTick, end.before(next));
            app.configure_sets(SimTick, PhaseMarker(next, true).after(end));
        }
        app.add_systems(
            SimTick,
            (
                marker(name, "start").in_set(start),
                marker(name, "end").in_set(end),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize)]
    struct Ping(u32);

    impl JournalRecord for Ping {
        const KIND: &'static str = "test.ping";
    }

    #[derive(Debug, Serialize)]
    struct Pong;

    impl JournalRecord for Pong {
        const KIND: &'static str = "test.pong";
    }

    fn journal() -> Journal {
        let mut kinds = StableRegistry::new(fl_protocol::tables::JOURNAL_RECORDS);
        kinds.register(Ping::KIND, "test", ());
        kinds.register(Pong::KIND, "test", ());
        kinds.freeze();
        let mut journal = Journal::new(kinds);
        journal.set_enabled(true);
        journal
    }

    #[test]
    fn take_sorted_orders_and_ranks_within_groups() {
        let mut j = journal();
        let t = Tick(3);
        j.record(t, Phase::Think, Some(SimId(2)), None, &Pong);
        j.record(t, Phase::Think, Some(SimId(2)), None, &Ping(1));
        j.record(t, Phase::Input, None, None, &Pong);
        j.record(t, Phase::Think, Some(SimId(1)), None, &Ping(2));
        j.record(t, Phase::Think, Some(SimId(2)), None, &Ping(3));
        let taken = j.take_sorted();
        assert!(j.is_empty());
        let keys: Vec<(u8, &str, Option<SimId>, u32)> = taken
            .iter()
            .map(|e| (e.phase, e.kind.as_str(), e.sim_id, e.emitted))
            .collect();
        assert_eq!(
            keys,
            [
                (0, "test.pong", None, 0),
                (1, "test.ping", Some(SimId(1)), 0),
                (1, "test.ping", Some(SimId(2)), 0),
                (1, "test.ping", Some(SimId(2)), 1),
                (1, "test.pong", Some(SimId(2)), 0),
            ]
        );
        assert_eq!(taken[2].data, serde_json::json!(1), "emission order kept");
        assert_eq!(taken[3].data, serde_json::json!(3));
    }

    #[test]
    fn disabled_journal_keeps_nothing() {
        let mut j = journal();
        j.set_enabled(false);
        j.record(Tick(0), Phase::Input, None, None, &Ping(0));
        assert_eq!(j.len(), 0);
    }

    #[test]
    #[should_panic(expected = "'test.unknown' is not registered")]
    #[cfg(debug_assertions)]
    fn unregistered_kind_panics_in_debug() {
        #[derive(Debug, Serialize)]
        struct Unknown;
        impl JournalRecord for Unknown {
            const KIND: &'static str = "test.unknown";
        }
        journal().record(Tick(0), Phase::Input, None, None, &Unknown);
    }
}
