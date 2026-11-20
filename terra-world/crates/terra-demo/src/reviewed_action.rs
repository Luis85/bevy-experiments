//! Presentation-only context shared by native operational reviews.
//! A review preserves intent; the domain still owns eligibility and mutation.
use std::sync::atomic::{AtomicU64, Ordering};
use terra_bevy::SandboxRuntime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeContext {
    player: u64,
    restore: u64,
    active_body: u64,
}

impl NativeContext {
    pub(crate) fn capture(s: &SandboxRuntime) -> Option<Self> {
        if !s.state.players.contains_key(&s.player_id) || s.state.is_player_dormant(s.player_id) {
            return None;
        }
        let body = s.state.living.active_unit(s.player_id)?;
        (body.owner == Some(s.player_id)).then_some(Self {
            player: s.player_id,
            restore: s.restore_revision,
            active_body: body.id,
        })
    }

    pub(crate) fn is_current(&self, s: &SandboxRuntime) -> bool {
        Self::capture(s).as_ref() == Some(self)
    }
}

/// Process-unique and nonwrapping, including after a UiState replacement.
pub(crate) fn next_token() -> Option<u64> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut candidate = NEXT.load(Ordering::Relaxed);
    loop {
        let next = candidate.checked_add(1)?;
        match NEXT.compare_exchange_weak(candidate, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Some(candidate),
            Err(current) => candidate = current,
        }
    }
}
