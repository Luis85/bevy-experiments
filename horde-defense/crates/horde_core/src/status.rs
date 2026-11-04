//! Generic timed status effects on simulation actors (defender attempt, enemies).
//! A status is active while it is stored; it is removed on the tick its clock
//! reaches zero, so "absent" and "0 ticks left" are the same state.
use serde::{Deserialize, Serialize};

/// Stable status identities. Serialized by name; ordering keeps the per-actor
/// list sorted so saves and iteration are deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StatusId {
    /// Enemy movement is held after positive injury; attacks keep their cadence.
    HitStop,
    /// The personal defender halves incoming damage.
    Brace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusEffect {
    pub id: StatusId,
    pub remaining_ticks: u32,
    pub stacks: u8,
    /// Status-specific strength; zero where the status has a fixed rule.
    pub magnitude: i32,
}

/// An actor's statuses, sorted by id with at most one entry per id and no
/// expired (zero-tick) entries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Statuses(Vec<StatusEffect>);

impl Statuses {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &StatusEffect> {
        self.0.iter()
    }
    pub fn get(&self, id: StatusId) -> Option<&StatusEffect> {
        self.0
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|n| &self.0[n])
    }
    /// Ticks left on `id`, or 0 when the status is not active.
    pub fn remaining(&self, id: StatusId) -> u32 {
        self.get(id).map_or(0, |s| s.remaining_ticks)
    }
    pub fn active(&self, id: StatusId) -> bool {
        self.remaining(id) > 0
    }
    /// Apply `effect`, refreshing an existing entry: the clock becomes the
    /// longer of the two, stacks the larger, and magnitude the new value.
    /// A zero-tick effect is a no-op.
    pub fn apply(&mut self, effect: StatusEffect) {
        if effect.remaining_ticks == 0 {
            return;
        }
        match self.0.binary_search_by_key(&effect.id, |s| s.id) {
            Ok(n) => {
                let current = &mut self.0[n];
                current.remaining_ticks = current.remaining_ticks.max(effect.remaining_ticks);
                current.stacks = current.stacks.max(effect.stacks);
                current.magnitude = effect.magnitude;
            }
            Err(n) => self.0.insert(n, effect),
        }
    }
    /// Apply a single-stack, fixed-rule status for `ticks`.
    pub fn refresh(&mut self, id: StatusId, ticks: u32) {
        self.apply(StatusEffect {
            id,
            remaining_ticks: ticks,
            stacks: 1,
            magnitude: 0,
        });
    }
    /// End `id` immediately, if active.
    pub fn remove(&mut self, id: StatusId) {
        self.0.retain(|s| s.id != id);
    }
    /// Advance every clock by one tick and drop the expired entries.
    pub fn tick(&mut self) {
        for status in &mut self.0 {
            status.remaining_ticks = status.remaining_ticks.saturating_sub(1);
        }
        self.0.retain(|s| s.remaining_ticks > 0);
    }
    /// Saved lists must be sorted, unique, unexpired, use only `allowed` ids,
    /// and stay within each id's maximum duration.
    pub fn validate(&self, allowed: &[(StatusId, u32)]) -> Result<(), String> {
        let sorted = self.0.windows(2).all(|w| w[0].id < w[1].id);
        let bounded = self.0.iter().all(|s| {
            s.remaining_ticks > 0
                && s.stacks > 0
                && allowed
                    .iter()
                    .any(|(id, max)| *id == s.id && s.remaining_ticks <= *max)
        });
        if sorted && bounded {
            Ok(())
        } else {
            Err("Saved status effects are unsorted, expired or out of bounds".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_stay_sorted_refresh_to_the_longer_clock_and_expire_on_zero() {
        let mut s = Statuses::default();
        s.refresh(StatusId::Brace, 3);
        s.refresh(StatusId::HitStop, 2);
        assert_eq!(
            s.iter().map(|e| e.id).collect::<Vec<_>>(),
            [StatusId::HitStop, StatusId::Brace]
        );
        s.refresh(StatusId::HitStop, 1);
        assert_eq!(s.remaining(StatusId::HitStop), 2);
        s.apply(StatusEffect {
            id: StatusId::Brace,
            remaining_ticks: 1,
            stacks: 3,
            magnitude: 7,
        });
        assert_eq!(
            s.get(StatusId::Brace),
            Some(&StatusEffect {
                id: StatusId::Brace,
                remaining_ticks: 3,
                stacks: 3,
                magnitude: 7
            })
        );
        s.tick();
        s.tick();
        assert!(!s.active(StatusId::HitStop));
        assert_eq!(s.iter().count(), 1);
        s.tick();
        assert!(s.is_empty());
        s.refresh(StatusId::Brace, 0);
        assert!(s.is_empty());
    }

    #[test]
    fn status_lists_match_a_saturating_counter_tick_for_tick() {
        // Equivalence with the replaced `left = left.max(n)` / `saturating_sub(1)`
        // counters, including re-application mid-countdown.
        let mut counter = 0u32;
        let mut statuses = Statuses::default();
        for tick in 0..40u32 {
            if tick % 7 == 0 || tick == 3 {
                counter = counter.max(6);
                statuses.refresh(StatusId::HitStop, 6);
            }
            assert_eq!(statuses.remaining(StatusId::HitStop), counter);
            assert_eq!(statuses.active(StatusId::HitStop), counter > 0);
            counter = counter.saturating_sub(1);
            statuses.tick();
        }
    }

    #[test]
    fn validation_rejects_unsorted_expired_unknown_and_overlong_entries() {
        let allowed = [(StatusId::HitStop, 6)];
        let effect = |id, remaining_ticks| StatusEffect {
            id,
            remaining_ticks,
            stacks: 1,
            magnitude: 0,
        };
        assert!(Statuses::default().validate(&allowed).is_ok());
        assert!(
            Statuses(vec![effect(StatusId::HitStop, 6)])
                .validate(&allowed)
                .is_ok()
        );
        for bad in [
            vec![effect(StatusId::HitStop, 7)],
            vec![effect(StatusId::HitStop, 0)],
            vec![effect(StatusId::Brace, 1)],
            vec![effect(StatusId::HitStop, 1), effect(StatusId::HitStop, 1)],
        ] {
            assert!(Statuses(bad).validate(&allowed).is_err());
        }
    }
}
