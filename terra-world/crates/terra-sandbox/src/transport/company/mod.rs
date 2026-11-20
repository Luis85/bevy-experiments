//! Owner-private commercial dispatch. Employees are contracts, not extra actors:
//! their assigned vehicles move through the ordinary authoritative road system.
use super::*;
use crate::catalog::{exchange, inventory};

mod accounting;
mod commands;
mod depots;
mod economy;
mod lifecycle;
mod market;
mod model;
mod planning;
mod roadside;
mod simulation;
mod validation;

pub use accounting::*;
pub use model::*;
pub use planning::*;
pub use simulation::fleet_fare;

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.len() <= 64
        && name.chars().count() <= 32
        && !name.chars().any(char::is_control)
}

fn body_speed(unit: &LivingUnit) -> f64 {
    unit.velocity_enu_m_s[0].hypot(unit.velocity_enu_m_s[1])
}
