//! Read-only player context shared by the compact HUD and Command inspector.
//! Selection is presentation state; no unavailable ID may borrow another body's
//! needs, and only an admitted vehicle/driver association can display a driver.
use crate::control::PlayerControl;
use terra_bevy::{
    SandboxRuntime,
    rules::{LivingUnit, UnitOrder},
};

/// Native display/activation identity. The domain Consume command remains an
/// active-body operation; this review is rechecked before native submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsumeTarget {
    player_id: u64,
    restore_revision: u64,
    body_id: u64,
}

impl ConsumeTarget {
    /// Nature and Activities name the active person directly. Their actions do
    /// not borrow the inspected selection, but retain the same owner/restore/
    /// body identity as Command consumption.
    pub(crate) fn active_person(sandbox: &SandboxRuntime) -> Option<Self> {
        if !sandbox.state.living.enabled
            || !sandbox.state.players.contains_key(&sandbox.player_id)
            || sandbox.state.is_player_dormant(sandbox.player_id)
        {
            return None;
        }
        let body = sandbox.state.living.active_unit(sandbox.player_id)?;
        (body.owner == Some(sandbox.player_id) && body.is_person()).then_some(Self {
            player_id: sandbox.player_id,
            restore_revision: sandbox.restore_revision,
            body_id: body.id,
        })
    }

    pub(crate) fn matches_active(self, sandbox: &SandboxRuntime) -> bool {
        Self::active_person(sandbox) == Some(self)
    }

    pub(crate) fn matches(self, sandbox: &SandboxRuntime, control: &PlayerControl) -> bool {
        self.matches_active(sandbox)
            && ActorSummary::resolve(sandbox, control).consume_body == Some(self.body_id)
    }
}

pub(crate) fn owned_selection<'a>(
    sandbox: &'a SandboxRuntime,
    control: &'a PlayerControl,
) -> impl Iterator<Item = u64> + 'a {
    control.selected.iter().copied().filter(|id| {
        sandbox
            .state
            .living
            .units
            .get(id)
            .is_some_and(|unit| unit.owner == Some(sandbox.player_id))
    })
}

#[derive(Debug)]
pub(crate) struct ActorSummary {
    pub detail: String,
    pub action_hint: String,
    pub needs: Option<[f32; 3]>,
    pub owned_selected: usize,
    /// Consumption still targets the domain account's active unit. This is
    /// present only when that exact body is the one, owned displayed selection.
    pub consume_body: Option<u64>,
    compact: String,
    need_prefix: &'static str,
}

impl ActorSummary {
    pub fn consume_target(&self, sandbox: &SandboxRuntime) -> Option<ConsumeTarget> {
        ConsumeTarget::active_person(sandbox)
            .filter(|target| self.consume_body == Some(target.body_id))
    }

    pub fn resolve(sandbox: &SandboxRuntime, control: &PlayerControl) -> Self {
        let life = &sandbox.state.living;
        let account = life.accounts.get(&sandbox.player_id);
        let ids: Vec<_> = if let Some(id) = control.possessed {
            vec![id]
        } else {
            control.selected.iter().copied().collect()
        };
        let action_hint = "Eat and drink affect your active person. Select that one owned body first. Stop and Rest affect your owned selection.";
        let mut result = Self {
            detail: String::new(),
            action_hint: action_hint.into(),
            needs: None,
            owned_selected: owned_selection(sandbox, control).count(),
            consume_body: None,
            compact: String::new(),
            need_prefix: "",
        };
        if ids.is_empty() {
            result.detail = "No actor selected.\nClick an actor to inspect; drag to select your crew. Command lists the owned actors in this view.".into();
            result.compact =
                "No actor selected · Click an actor or use Command to select your crew".into();
            return result;
        }
        if ids.len() > 1 {
            let missing = ids.iter().filter(|id| !life.units.contains_key(id)).count();
            let foreign = ids.len() - result.owned_selected - missing;
            let mut biological = 0;
            for id in &ids {
                if let Some(unit) = life.units.get(id)
                    && unit.owner == Some(sandbox.player_id)
                    && unit.is_person()
                {
                    biological += 1;
                    let values = values(unit);
                    result.needs = Some(match result.needs {
                        Some(old) => std::array::from_fn(|i| old[i].min(values[i])),
                        None => values,
                    });
                }
            }
            let mut scope = format!("{} selected · {} owned", ids.len(), result.owned_selected);
            if foreign != 0 {
                scope.push_str(&format!(" · {foreign} inspect only"));
            }
            if missing != 0 {
                scope.push_str(&format!(" · {missing} unavailable"));
            }
            result.need_prefix = if missing == 0 {
                "Lowest "
            } else {
                "Lowest shown "
            };
            result.detail = format!(
                "{scope}\n{biological} owned biological bodies · {} owned vehicles/other bodies\n{}",
                result.owned_selected - biological,
                if biological == 0 {
                    "No selected owned biological needs are available.".into()
                } else {
                    format!(
                        "{}needs across {biological} selected owned biological bodies.{}",
                        result.need_prefix,
                        if missing == 0 {
                            ""
                        } else {
                            " Missing bodies are not included."
                        }
                    )
                }
            );
            result.compact = if let Some(needs) = result.needs {
                format!(
                    "{scope} · {} ({biological} bodies): {}",
                    result.need_prefix.trim_end(),
                    needs_line(needs)
                )
            } else {
                format!("{scope} · No biological needs shown")
            };
            return result;
        }
        let id = ids[0];
        let Some(unit) = life.units.get(&id) else {
            result.detail = format!(
                "Selected actor #{id} is unavailable.\nThe current view has no data for this selection. Select an available actor to inspect or command."
            );
            result.compact =
                format!("Selected actor #{id} unavailable · Select an actor in the current view");
            return result;
        };
        let owned = unit.owner == Some(sandbox.player_id);
        let admitted = owned && account.is_some_and(|a| a.possessed && a.active_unit == id);
        let pending = control.possessed == Some(id) && !admitted;
        let ownership = if !owned {
            "Inspect only"
        } else if pending {
            "Awaiting control"
        } else if admitted {
            "Controlling"
        } else {
            "Your actor"
        };
        result.detail = format!(
            "{}\n{:?} · {ownership}\n{}",
            unit.name,
            unit.kind,
            order_line(unit)
        );
        result.compact = format!("{} · {ownership}", compact_name(&unit.name));
        if unit.is_person() {
            result.needs = Some(values(unit));
            if owned
                && !pending
                && control.selected.len() == 1
                && control.selected.contains(&id)
                && account.is_some_and(|a| a.active_unit == id)
            {
                result.consume_body = Some(id);
                result.action_hint = format!(
                    "Eat and drink affect {}, your active body. Stop and Rest affect your owned selection.",
                    unit.name
                );
            }
        } else if sandbox.state.transport.vehicles.contains_key(&id) && admitted {
            if let Some(driver) = account
                .and_then(|a| life.units.get(&a.avatar_unit))
                .filter(|u| u.owner == Some(sandbox.player_id) && u.is_person())
            {
                result.needs = Some(values(driver));
                result.need_prefix = "Driver ";
                result
                    .detail
                    .push_str(&format!("\nDriver: {}", driver.name));
                result
                    .compact
                    .push_str(&format!(" · Driver: {}", compact_name(&driver.name)));
                result.action_hint = "These are the driver's needs. Park and exit to eat, drink or rest. Stop and Rest affect your owned selection; vehicle Rest only stops the vehicle and does not replenish driver needs.".into();
            } else {
                result
                    .detail
                    .push_str("\nDriver needs unavailable in this view.");
                result.compact.push_str(" · Driver needs unavailable");
            }
        } else {
            result
                .detail
                .push_str("\nThis body has no biological needs.");
            result.compact.push_str(" · No biological needs");
        }
        if let Some(needs) = result.needs {
            result
                .compact
                .push_str(&format!(" · {}", needs_line(needs)));
        }
        result
    }

    pub fn footer(&self, remote: bool, partial: bool) -> String {
        format!("{}{}", self.compact, view_scope(remote, partial))
    }

    pub fn inspector(&self, remote: bool, partial: bool) -> String {
        if remote {
            format!(
                "{}\n{}",
                self.detail,
                if partial {
                    "Partial received view; other actors may be absent."
                } else {
                    "Received actors only; this is not a whole-world roster."
                }
            )
        } else {
            self.detail.clone()
        }
    }

    pub fn need_label(&self, index: usize) -> String {
        let name = ["Food", "Water", "Energy"][index];
        self.needs.map_or_else(
            || format!("{name}   —"),
            |values| {
                format!(
                    "{}{name}   {:.0}%{}",
                    self.need_prefix,
                    values[index] * 100.,
                    if values[index] < 0.25 { " · Low" } else { "" }
                )
            },
        )
    }
}

fn values(unit: &LivingUnit) -> [f32; 3] {
    [unit.needs.food, unit.needs.water, unit.needs.energy]
}

fn needs_line(values: [f32; 3]) -> String {
    format!(
        "Food {:.0}% · Water {:.0}% · Energy {:.0}%",
        values[0] * 100.,
        values[1] * 100.,
        values[2] * 100.
    )
}

fn compact_name(name: &str) -> String {
    let mut characters = name.chars();
    let compact: String = characters.by_ref().take(24).collect();
    if characters.next().is_some() {
        format!("{compact}…")
    } else {
        compact
    }
}

fn view_scope(remote: bool, partial: bool) -> &'static str {
    if !remote {
        ""
    } else if partial {
        " · Partial received view"
    } else {
        " · Received view"
    }
}

fn order_line(unit: &LivingUnit) -> String {
    match unit.order {
        UnitOrder::Idle => "Ready".into(),
        UnitOrder::Rest => "Resting".into(),
        UnitOrder::Patrol => "On route".into(),
        UnitOrder::Move { destination } => format!(
            "Moving · {:.0} m",
            unit.position.surface_distance_m(destination)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::ControlMode;
    use std::collections::BTreeSet;
    use terra_bevy::rules::Needs;
    use terra_core::{ActorKind, GeoPosition};

    fn fixture() -> (SandboxRuntime, PlayerControl, u64, u64) {
        let position = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let mut sandbox = SandboxRuntime::new(position);
        sandbox
            .state
            .seed_living_demo(position, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        let person = sandbox.state.living.accounts[&sandbox.player_id].avatar_unit;
        let vehicle = sandbox
            .state
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == sandbox.player_id)
            .unwrap()
            .unit_id;
        let mut control = PlayerControl::default();
        control.selected.insert(person);
        (sandbox, control, person, vehicle)
    }

    #[test]
    fn empty_and_missing_selection_never_borrow_active_body_needs() {
        let (sandbox, mut control, person, _) = fixture();
        control.selected.clear();
        let empty = ActorSummary::resolve(&sandbox, &control);
        assert!(empty.detail.starts_with("No actor selected"));
        assert_eq!(empty.needs, None);
        assert_eq!(empty.consume_body, None);
        control.selected.insert(u64::MAX);
        let missing = ActorSummary::resolve(&sandbox, &control);
        assert!(missing.detail.contains("unavailable"));
        assert!(
            !missing
                .detail
                .contains(&sandbox.state.living.units[&person].name)
        );
        assert_eq!(missing.needs, None);
        assert_eq!(missing.owned_selected, 0);
    }

    #[test]
    fn single_own_and_foreign_bodies_are_explicit_and_only_active_owned_body_can_consume() {
        let (mut sandbox, control, person, _) = fixture();
        sandbox.state.living.units.get_mut(&person).unwrap().needs = Needs {
            food: 0.2,
            water: 0.5,
            energy: 0.7,
        };
        let own = ActorSummary::resolve(&sandbox, &control);
        assert_eq!(own.needs, Some([0.2, 0.5, 0.7]));
        assert_eq!(own.consume_body, Some(person));
        assert!(own.need_label(0).contains("Low"));
        sandbox.state.living.units.get_mut(&person).unwrap().owner = Some(2);
        let foreign = ActorSummary::resolve(&sandbox, &control);
        assert!(foreign.detail.contains("Inspect only"));
        assert_eq!(foreign.needs, own.needs);
        assert_eq!(foreign.consume_body, None);
    }

    #[test]
    fn group_minima_exclude_non_owned_and_nonbiological_bodies_and_mark_missing_data() {
        let (mut sandbox, mut control, person, vehicle) = fixture();
        let other = sandbox
            .state
            .living
            .units
            .values()
            .find(|u| u.owner == Some(1) && u.is_person() && u.id != person)
            .unwrap()
            .id;
        let foreign = sandbox
            .state
            .living
            .units
            .values()
            .find(|u| u.owner != Some(1) && u.is_person())
            .unwrap()
            .id;
        sandbox.state.living.units.get_mut(&person).unwrap().needs = Needs {
            food: 0.8,
            water: 0.3,
            energy: 0.6,
        };
        sandbox.state.living.units.get_mut(&other).unwrap().needs = Needs {
            food: 0.4,
            water: 0.9,
            energy: 0.2,
        };
        for id in [vehicle, foreign] {
            sandbox.state.living.units.get_mut(&id).unwrap().needs = Needs {
                food: 0.,
                water: 0.,
                energy: 0.,
            };
        }
        control.selected = BTreeSet::from([person, other, vehicle, foreign, u64::MAX]);
        let group = ActorSummary::resolve(&sandbox, &control);
        assert_eq!(group.needs, Some([0.4, 0.3, 0.2]));
        assert_eq!(group.consume_body, None);
        assert_eq!(group.owned_selected, 3);
        for wording in [
            "5 selected",
            "3 owned",
            "1 inspect only",
            "1 unavailable",
            "Lowest shown",
            "2 selected owned biological",
        ] {
            assert!(
                group.detail.contains(wording),
                "{wording}: {}",
                group.detail
            );
        }
        assert!(group.need_label(2).starts_with("Lowest shown Energy"));
        assert!(group.footer(true, true).ends_with("Partial received view"));
        assert!(
            group
                .inspector(true, false)
                .contains("not a whole-world roster")
        );
    }

    #[test]
    fn only_the_exact_admitted_owned_vehicle_uses_its_owned_biological_driver() {
        let (mut sandbox, mut control, person, vehicle) = fixture();
        control.selected = BTreeSet::from([vehicle]);
        control.mode = ControlMode::ThirdPerson;
        control.possessed = Some(vehicle);
        assert_eq!(
            ActorSummary::resolve(&sandbox, &control).needs,
            None,
            "optimistic possession is not driver admission"
        );
        {
            let account = sandbox.state.living.accounts.get_mut(&1).unwrap();
            account.active_unit = vehicle;
            account.possessed = true;
        }
        let admitted = ActorSummary::resolve(&sandbox, &control);
        assert!(admitted.detail.contains("Driver:"));
        assert_eq!(
            admitted.needs,
            Some(values(&sandbox.state.living.units[&person]))
        );
        assert_eq!(admitted.consume_body, None);
        assert!(admitted.need_label(0).starts_with("Driver Food"));
        sandbox.state.living.units.get_mut(&vehicle).unwrap().owner = Some(2);
        assert_eq!(ActorSummary::resolve(&sandbox, &control).needs, None);
        sandbox.state.living.units.get_mut(&vehicle).unwrap().owner = Some(1);
        sandbox.state.living.units.get_mut(&person).unwrap().owner = Some(2);
        assert_eq!(ActorSummary::resolve(&sandbox, &control).needs, None);
        sandbox.state.living.units.get_mut(&person).unwrap().owner = Some(1);
        sandbox.state.living.units.get_mut(&person).unwrap().kind = ActorKind::Vehicle;
        assert_eq!(ActorSummary::resolve(&sandbox, &control).needs, None);
        sandbox.state.living.units.remove(&person);
        let absent = ActorSummary::resolve(&sandbox, &control);
        assert!(absent.detail.contains("Driver needs unavailable"));
        assert_eq!(absent.needs, None);
    }

    #[test]
    fn selecting_another_vehicle_does_not_borrow_the_current_drivers_needs() {
        let (mut sandbox, mut control, _, vehicle) = fixture();
        let second = vehicle + 1_000_000;
        let mut another = sandbox.state.living.units[&vehicle].clone();
        another.id = second;
        sandbox.state.living.units.insert(second, another);
        let mut registration = sandbox.state.transport.vehicles[&vehicle].clone();
        registration.unit_id = second;
        sandbox
            .state
            .transport
            .vehicles
            .insert(second, registration);
        let account = sandbox.state.living.accounts.get_mut(&1).unwrap();
        account.active_unit = vehicle;
        account.possessed = true;
        control.selected = BTreeSet::from([second]);
        let other = ActorSummary::resolve(&sandbox, &control);
        assert_eq!(other.needs, None);
        assert!(!other.detail.contains("Driver:"));
    }

    #[test]
    fn compact_names_are_unicode_safe_and_full_inspection_retains_identity() {
        let (mut sandbox, control, person, _) = fixture();
        let name = "É".repeat(32);
        sandbox.state.living.units.get_mut(&person).unwrap().name = name.clone();
        let summary = ActorSummary::resolve(&sandbox, &control);
        assert!(summary.detail.starts_with(&name));
        assert!(
            summary
                .footer(false, false)
                .starts_with(&format!("{}…", "É".repeat(24)))
        );
        assert!(!summary.footer(false, true).contains("received"));
    }
}
