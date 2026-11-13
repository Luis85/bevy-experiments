//! The Residents tab's roster: one row per living resident, sorted and
//! filtered in the view. Read-only; choosing a row selects that resident for
//! the inspector and moves the camera, nothing else.
use crate::resident_inspector::{self, model::NeedLevel};
use bevy::prelude::*;
use npc_framework::prelude::{SimId, SimIndex};
use testbed_content::village::{self, Role, Villager, Wallet, colony};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SortKey {
    #[default]
    Name,
    Settlement,
    Profession,
    /// Lowest morale first: the people who need a look.
    Mood,
    /// Most coins first.
    Wealth,
}

impl SortKey {
    pub(crate) const ALL: [Self; 5] = [
        Self::Name,
        Self::Settlement,
        Self::Profession,
        Self::Mood,
        Self::Wealth,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Settlement => "Place",
            Self::Profession => "Job",
            Self::Mood => "Mood",
            Self::Wealth => "Wealth",
        }
    }
    /// What the order means, for the line under the choices.
    pub(crate) fn note(self) -> &'static str {
        match self {
            Self::Name => "by name",
            Self::Settlement => "by settlement, then name",
            Self::Profession => "by profession, then name",
            Self::Mood => "lowest mood first",
            Self::Wealth => "richest first",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RosterRow {
    pub(crate) id: SimId,
    pub(crate) name: String,
    pub(crate) settlement: Option<SimId>,
    pub(crate) place: String,
    pub(crate) profession: String,
    /// Morale in milli-units (0..=100000).
    pub(crate) mood: i32,
    pub(crate) coins: u32,
    pub(crate) activity: String,
    /// The most pressing bodily need.
    pub(crate) need: NeedLevel,
}

impl RosterRow {
    /// Two lines: who and where, then what they are doing and how they fare.
    pub(crate) fn text(&self) -> String {
        let mut who = format!("{} · {}", self.name, self.profession);
        if !self.place.is_empty() {
            who.push_str(&format!(" · {}", self.place));
        }
        let need = match self.need {
            NeedLevel::Fine => "",
            NeedLevel::Low => " · in need",
            NeedLevel::Critical => " · in danger",
        };
        format!(
            "{who}\n{} · mood {}% · {} coin{}{need}",
            self.activity,
            (self.mood / 1000).clamp(0, 100),
            self.coins,
            if self.coins == 1 { "" } else { "s" },
        )
    }
}

/// Keep the residents of `scope` (`None`: everyone) and order them by `sort`.
/// Every order ends in name, then id, so equal rows never swap places.
pub(crate) fn arrange(
    mut rows: Vec<RosterRow>,
    sort: SortKey,
    scope: Option<SimId>,
) -> Vec<RosterRow> {
    if scope.is_some() {
        rows.retain(|row| row.settlement == scope);
    }
    rows.sort_by(|a, b| {
        let primary = match sort {
            SortKey::Name => std::cmp::Ordering::Equal,
            SortKey::Settlement => a.place.cmp(&b.place),
            SortKey::Profession => a.profession.cmp(&b.profession),
            SortKey::Mood => a.mood.cmp(&b.mood),
            SortKey::Wealth => b.coins.cmp(&a.coins),
        };
        primary
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    rows
}

/// Every living resident; the played visitor is not a resident.
pub(crate) fn rows(world: &World) -> Vec<RosterRow> {
    let Some(index) = world.get_resource::<SimIndex>() else {
        return Vec::new();
    };
    index
        .map
        .iter()
        .filter_map(|(id, entity)| {
            let person = world.get::<Villager>(*entity)?;
            if person.role == Role::Visitor || !village::living(world, *entity) {
                return None;
            }
            Some(RosterRow {
                id: *id,
                name: person.name.clone(),
                settlement: village::frontier::settlements::member(world, *entity),
                place: resident_inspector::settlement_name(world, *entity).unwrap_or_default(),
                // By the trade's display name: a data-only trade too.
                profession: super::economy::trade_display(world, person.role).to_lowercase(),
                mood: world
                    .get::<colony::SocialLife>(*entity)
                    .map_or(75_000, |social| social.morale),
                coins: world
                    .get::<Wallet>(*entity)
                    .map_or(0, |wallet| wallet.coins),
                activity: resident_inspector::activity_label(world, *entity),
                need: resident_inspector::worst_need(world, *entity),
            })
        })
        .collect()
}

/// "waiting for planks" as a sentence start.
pub(crate) fn capitalised(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}
