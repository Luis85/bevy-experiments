use horde_core::{
    content::Mode,
    ordnance::{self, OrdnanceKind},
    sim::Attempt,
    skill_catalog::unlocks,
    world::StageTheme,
};

/// The defender's magazine and reload clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponView {
    pub ammo: u32,
    pub magazine: u32,
    pub reload_left: u32,
    pub reload_total: u32,
}
impl WeaponView {
    pub fn of(a: &Attempt) -> Self {
        Self {
            ammo: a.ammo,
            magazine: a.magazine,
            reload_left: a.reload_left,
            reload_total: a.reload_duration_ticks(),
        }
    }
    /// Reload completion in 0..=1 while reloading.
    pub fn reload_progress(&self) -> Option<f32> {
        (self.reload_left > 0).then(|| {
            (1.0 - self.reload_left as f32 / self.reload_total.max(1) as f32).clamp(0.0, 1.0)
        })
    }
}

/// One throwable's unlock, stock and cooldown for the attack selector, the
/// target cursor and the reticle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrdnanceSlotView {
    pub unlocked: bool,
    pub charges: u32,
    pub issued: u32,
    pub cooldown: u32,
    /// Full cooldown in ticks after the profile multiplier, at least 1.
    pub cooldown_total: f32,
    pub ready: bool,
    /// Blast radius in yards after the profile multiplier.
    pub radius: f32,
}
impl OrdnanceSlotView {
    pub fn of(a: &Attempt, kind: OrdnanceKind) -> Self {
        let definition = ordnance::catalog().definition(kind);
        let modifiers = &a.profile.modifiers;
        Self {
            unlocked: modifiers.unlocked(match kind {
                OrdnanceKind::Grenade => unlocks::GRENADE,
                OrdnanceKind::Molotov => unlocks::MOLOTOV,
            }),
            charges: a.ordnance.charges(kind),
            issued: a.ordnance.issued(kind),
            cooldown: a.ordnance.cooldown(kind),
            cooldown_total: (definition.cooldown_ticks as f32
                * modifiers.ordnance_cooldown_multiplier)
                .round()
                .clamp(1.0, 3600.0),
            ready: a.ordnance.ready(kind),
            radius: definition.radius * modifiers.ordnance_radius_multiplier,
        }
    }
}

/// The cursor badge: rounds or charges left, a reload or cooldown arc and a
/// short label.
#[derive(Clone, Debug, PartialEq)]
pub struct ReticleReadout {
    pub remaining: u32,
    pub capacity: u32,
    pub progress: Option<f32>,
    pub label: String,
    /// Observed (NPC) battles draw the neutral reticle colour.
    pub observed: bool,
}
impl ReticleReadout {
    /// The readout for `a`; a selected throwable applies to personal battles only.
    pub fn build(a: &Attempt, selected: Option<OrdnanceKind>) -> Self {
        let slot = selected.map(|kind| OrdnanceSlotView::of(a, kind));
        Self::from_parts(a.mode, &WeaponView::of(a), slot.as_ref())
    }
    pub(super) fn from_parts(
        mode: Mode,
        weapon: &WeaponView,
        slot: Option<&OrdnanceSlotView>,
    ) -> Self {
        let reload = weapon.reload_progress();
        let observed = mode == Mode::Npc;
        match slot.filter(|_| mode == Mode::Personal) {
            Some(slot) => Self {
                remaining: slot.charges,
                capacity: slot.issued.max(1),
                progress: reload.or((slot.cooldown > 0)
                    .then(|| (1.0 - slot.cooldown as f32 / slot.cooldown_total).clamp(0.0, 1.0))),
                label: if reload.is_some() {
                    "RELOAD".into()
                } else {
                    format!("{} left", slot.charges)
                },
                observed,
            },
            None => Self {
                remaining: weapon.ammo,
                capacity: weapon.magazine.max(1),
                progress: reload,
                label: if reload.is_some() {
                    format!("R {:.1}s", weapon.reload_left as f32 / 60.0)
                } else {
                    format!("{}/{}", weapon.ammo, weapon.magazine)
                },
                observed,
            },
        }
    }
}

/// Which first-deployment field hint applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldHint {
    /// Nothing has happened yet.
    Opening,
    /// No shot fired.
    Shoot,
    /// Never reloaded and the magazine is not full.
    Reload,
    /// No enemy has reached the line.
    Contact,
    Banked,
}

/// Deployment facts for the field-details menu.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldReadout {
    pub threat_rank: u32,
    pub hostiles: usize,
    pub enemy_cap: usize,
    pub arrival_rate: f32,
    pub difficulty: &'static str,
    pub batch_size: u32,
    pub gear_level: u32,
    pub level_name: &'static str,
    pub hint: FieldHint,
    /// The country stage's local setting, when deployed to a country.
    pub setting: Option<StageTheme>,
}

/// Lines of the resolution log the field-details menu shows.
pub const LOG_TAIL: usize = 18;
