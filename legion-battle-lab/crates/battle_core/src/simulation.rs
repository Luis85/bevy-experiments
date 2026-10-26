use crate::spatial::{ActivityGrid, SpatialGrid};
use crate::terrain::{constrain_move, route_direction, speed_factor};
use crate::{
    Formation, Point, SIM_DT, SIM_HZ, ScenarioConfig, Team, UnitCategory, UnitKind,
    arena_half_extent,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Dense structure-of-arrays storage. The array index is a stable UnitId for
/// this battle; dead slots are retained rather than recycled under stale IDs.
#[derive(Clone, Debug)]
pub struct UnitStore {
    pub positions: Vec<Point>,
    pub previous_positions: Vec<Point>,
    pub velocities: Vec<Point>,
    pub headings: Vec<f32>,
    pub teams: Vec<Team>,
    pub kinds: Vec<UnitKind>,
    pub categories: Vec<UnitCategory>,
    pub health: Vec<f32>,
    pub alive: Vec<bool>,
    pub attack_flash: Vec<f32>,
}

impl UnitStore {
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Allocated vector storage only; useful for distinguishing real state
    /// capacity from renderer, snapshot, allocator and process overhead.
    pub fn allocated_bytes(&self) -> usize {
        (self.positions.capacity()
            + self.previous_positions.capacity()
            + self.velocities.capacity())
            * size_of::<Point>()
            + (self.headings.capacity() + self.health.capacity() + self.attack_flash.capacity())
                * size_of::<f32>()
            + self.teams.capacity() * size_of::<Team>()
            + self.kinds.capacity() * size_of::<UnitKind>()
            + self.categories.capacity() * size_of::<UnitCategory>()
            + self.alive.capacity() * size_of::<bool>()
    }

    fn with_capacity(count: usize) -> Self {
        Self {
            positions: Vec::with_capacity(count),
            previous_positions: Vec::with_capacity(count),
            velocities: Vec::with_capacity(count),
            headings: Vec::with_capacity(count),
            teams: Vec::with_capacity(count),
            kinds: Vec::with_capacity(count),
            categories: Vec::with_capacity(count),
            health: Vec::with_capacity(count),
            alive: Vec::with_capacity(count),
            attack_flash: Vec::with_capacity(count),
        }
    }

    fn push(&mut self, position: Point, team: Team, kind: UnitKind, category: UnitCategory) {
        self.positions.push(position);
        self.previous_positions.push(position);
        self.velocities.push(Point::ZERO);
        self.headings.push(if team == Team::Blue {
            0.0
        } else {
            std::f32::consts::PI
        });
        self.teams.push(team);
        self.kinds.push(kind);
        self.categories.push(category);
        self.health.push(category.definition(kind).health);
        self.alive.push(true);
        self.attack_flash.push(0.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Victory(Team),
    Draw,
    TimeLimit,
}

impl Outcome {
    pub fn label(self) -> String {
        match self {
            Self::Victory(team) => format!("{} gewinnt", team.label()),
            Self::Draw => "Unentschieden".into(),
            Self::TimeLimit => "Zeitlimit erreicht".into(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BattleStats {
    pub tick: u64,
    pub elapsed_secs: f32,
    pub alive: [u32; 2],
    pub casualties: [u32; 2],
    /// Candidate visits in the last tick, including separation and targeting.
    pub neighbor_checks: u64,
    pub max_cell_occupancy: usize,
    /// Living units evaluated with full local combat and crowd behavior.
    pub full_detail_units: u32,
    /// Living units using camera-independent strategic steering this tick.
    pub strategic_units: u32,
    /// Persistent backend allocation estimate; excludes renderer and snapshots.
    pub estimated_state_bytes: usize,
    /// Measured CPU wall time. This diagnostic never influences gameplay.
    pub step_ms: f32,
}

pub struct Battle {
    pub config: ScenarioConfig,
    pub units: UnitStore,
    pub stats: BattleStats,
    pub outcome: Option<Outcome>,
    grid: SpatialGrid,
    activity_grid: Option<ActivityGrid>,
    strategic_velocities: Vec<Point>,
    next_positions: Vec<Point>,
    damage: Vec<f32>,
    pending_hits: Vec<PendingHit>,
    worker_count: usize,
    cooldowns: Vec<f32>,
    extent: f32,
}

#[derive(Clone, Copy)]
struct ArmyBounds {
    center: Point,
    minimum: Point,
    maximum: Point,
}

impl ArmyBounds {
    fn distance_squared(self, point: Point) -> f32 {
        let closest = Point::new(
            point.x.clamp(self.minimum.x, self.maximum.x),
            point.y.clamp(self.minimum.y, self.maximum.y),
        );
        point.distance_squared(closest)
    }

    fn march_goal(self, point: Point) -> Point {
        let closest = Point::new(
            point.x.clamp(self.minimum.x, self.maximum.x),
            point.y.clamp(self.minimum.y, self.maximum.y),
        );
        if closest.distance_squared(point) < 1.0 {
            self.center
        } else {
            closest
        }
    }
}

impl Battle {
    pub fn new(config: ScenarioConfig) -> Result<Self, String> {
        config.validate()?;
        let count = config.total_units() as usize;
        let extent = arena_half_extent(&config);
        let use_activity_lod = count > 20_000;
        let mut units = UnitStore::with_capacity(count);
        let gap = (8.0 + (config.blue.count.max(config.red.count) as f32).sqrt() * 0.10).min(24.0);
        for (team, army) in [(Team::Blue, &config.blue), (Team::Red, &config.red)] {
            let direction = if team == Team::Blue { -1.0 } else { 1.0 };
            let spacing = army.category.spawn_spacing(army.kind);
            for index in 0..army.count {
                let (row, column) = formation_slot(index, army.count, army.formation);
                let id = units.len() as u64;
                let jitter_x = (random(config.seed, id, 0) - 0.5) * 0.10;
                let jitter_z = (random(config.seed, id, 1) - 0.5) * 0.10;
                let position = Point::new(
                    direction * (gap + row * spacing) + jitter_x,
                    column * spacing + jitter_z,
                );
                units.push(position, team, army.kind, army.category);
            }
        }
        let cooldowns = (0..count)
            .map(|i| {
                random(config.seed, i as u64, 2)
                    * units.categories[i].definition(units.kinds[i]).attack_period
            })
            .collect();
        let next_positions = units.positions.clone();
        let stats = BattleStats {
            alive: [config.blue.count, config.red.count],
            ..Default::default()
        };
        let mut battle = Self {
            config,
            units,
            stats,
            outcome: None,
            grid: SpatialGrid::new(extent, count),
            activity_grid: use_activity_lod.then(|| ActivityGrid::new(extent)),
            strategic_velocities: if use_activity_lod {
                vec![Point::ZERO; count]
            } else {
                Vec::new()
            },
            next_positions,
            damage: vec![0.0; count],
            pending_hits: vec![PendingHit::NONE; count],
            // Cap worker overhead and avoid threads for small batches. This
            // value affects scheduling only, never the resulting game state.
            worker_count: if use_activity_lod {
                std::thread::available_parallelism()
                    .map_or(1, |workers| workers.get().min(8).min(count / 32_768).max(1))
            } else {
                1
            },
            cooldowns,
            extent,
        };
        battle.stats.estimated_state_bytes = battle.allocated_bytes();
        Ok(battle)
    }

    pub fn unit_health_fraction(&self, index: usize) -> f32 {
        if index >= self.units.len() {
            return 0.0;
        }
        (self.units.health[index]
            / self.units.categories[index]
                .definition(self.units.kinds[index])
                .health)
            .clamp(0.0, 1.0)
    }

    pub fn allocated_bytes(&self) -> usize {
        self.units.allocated_bytes()
            + (self.next_positions.capacity() + self.strategic_velocities.capacity())
                * size_of::<Point>()
            + (self.damage.capacity() + self.cooldowns.capacity()) * size_of::<f32>()
            + self.pending_hits.capacity() * size_of::<PendingHit>()
            + self.grid.allocated_bytes()
            + self
                .activity_grid
                .as_ref()
                .map_or(0, ActivityGrid::allocated_bytes)
    }

    /// Advance exactly one tick. All targeting and movement decisions read the
    /// same old snapshot, all damage commits afterwards. A killed attacker may
    /// therefore still land a strike it initiated in this tick.
    pub fn step(&mut self) {
        if self.outcome.is_some() {
            return;
        }
        let started = Instant::now();
        self.grid.rebuild(&self.units);
        if let Some(activity) = &mut self.activity_grid {
            activity.rebuild(&self.units);
        }
        self.damage.fill(0.0);
        self.stats.neighbor_checks = 0;
        self.stats.max_cell_occupancy = self.grid.max_occupancy;
        self.stats.full_detail_units = 0;
        self.stats.strategic_units = 0;
        let armies = self.army_bounds();

        // Each worker reads the same positions, velocities and unit traits.
        // Only disjoint output slices are mutable; no atomics or locks affect
        // combat. Pending hits are reduced in stable attacker-ID order below.
        let context = UnitKernel {
            units: ReadUnits {
                positions: &self.units.positions,
                velocities: &self.units.velocities,
                teams: &self.units.teams,
                kinds: &self.units.kinds,
                categories: &self.units.categories,
                alive: &self.units.alive,
            },
            grid: &self.grid,
            activity_grid: self.activity_grid.as_ref(),
            armies,
            seed: self.config.seed,
            tick: self.stats.tick,
            map: self.config.map,
            extent: self.extent,
        };
        let batch = UnitBatch {
            start: 0,
            next_positions: &mut self.next_positions,
            strategic_velocities: &mut self.strategic_velocities,
            cooldowns: &mut self.cooldowns,
            headings: &mut self.units.headings,
            attack_flash: &mut self.units.attack_flash,
            pending_hits: &mut self.pending_hits,
        };
        let stats = if self.worker_count > 1 {
            evaluate_parallel(&context, batch, self.worker_count)
        } else {
            evaluate_batch(&context, batch)
        };
        self.stats.neighbor_checks = stats.neighbor_checks;
        self.stats.full_detail_units = stats.full_detail_units;
        self.stats.strategic_units = stats.strategic_units;

        // Floating-point additions retain precisely the original order even
        // when attackers run on different workers or worker counts differ.
        for hit in &self.pending_hits {
            if hit.target != u32::MAX {
                self.damage[hit.target as usize] += hit.damage;
            }
        }

        for i in 0..self.units.len() {
            self.units.previous_positions[i] = self.units.positions[i];
            self.units.positions[i] = self.next_positions[i];
            if !self.units.alive[i] {
                self.units.velocities[i] = Point::ZERO;
                continue;
            }
            self.units.velocities[i] =
                (self.units.positions[i] - self.units.previous_positions[i]) / SIM_DT;
            self.units.health[i] = (self.units.health[i] - self.damage[i]).max(0.0);
            if self.units.health[i] <= 0.0 {
                self.units.alive[i] = false;
                self.units.velocities[i] = Point::ZERO;
                let team = self.units.teams[i].index();
                self.stats.alive[team] -= 1;
                self.stats.casualties[team] += 1;
            }
        }
        self.stats.tick += 1;
        self.stats.elapsed_secs = (self.stats.tick as f64 / SIM_HZ) as f32;
        self.outcome = match self.stats.alive {
            [0, 0] => Some(Outcome::Draw),
            [0, _] => Some(Outcome::Victory(Team::Red)),
            [_, 0] => Some(Outcome::Victory(Team::Blue)),
            _ if self.stats.tick >= self.config.time_limit_secs as u64 * SIM_HZ as u64 => {
                Some(Outcome::TimeLimit)
            }
            _ => None,
        };
        self.stats.step_ms = started.elapsed().as_secs_f32() * 1000.0;
    }

    fn army_bounds(&self) -> [ArmyBounds; 2] {
        let mut bounds = [ArmyBounds {
            center: Point::ZERO,
            minimum: Point::new(f32::INFINITY, f32::INFINITY),
            maximum: Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
        }; 2];
        for i in 0..self.units.len() {
            if self.units.alive[i] {
                let point = self.units.positions[i];
                let army = &mut bounds[self.units.teams[i].index()];
                army.center += point;
                army.minimum.x = army.minimum.x.min(point.x);
                army.minimum.y = army.minimum.y.min(point.y);
                army.maximum.x = army.maximum.x.max(point.x);
                army.maximum.y = army.maximum.y.max(point.y);
            }
        }
        for (team, army) in bounds.iter_mut().enumerate() {
            army.center = army.center / self.stats.alive[team].max(1) as f32;
        }
        bounds
    }
}

#[derive(Clone, Copy)]
struct PendingHit {
    target: u32,
    damage: f32,
}

impl PendingHit {
    const NONE: Self = Self {
        target: u32::MAX,
        damage: 0.0,
    };
}

/// Immutable fields needed by every action. Headings and attack flashes are
/// visual outputs and can be updated in disjoint slices during evaluation.
struct ReadUnits<'a> {
    positions: &'a [Point],
    velocities: &'a [Point],
    teams: &'a [Team],
    kinds: &'a [UnitKind],
    categories: &'a [UnitCategory],
    alive: &'a [bool],
}

struct UnitKernel<'a> {
    units: ReadUnits<'a>,
    grid: &'a SpatialGrid,
    activity_grid: Option<&'a ActivityGrid>,
    armies: [ArmyBounds; 2],
    seed: u64,
    tick: u64,
    map: crate::MapKind,
    extent: f32,
}

struct UnitBatch<'a> {
    start: usize,
    next_positions: &'a mut [Point],
    strategic_velocities: &'a mut [Point],
    cooldowns: &'a mut [f32],
    headings: &'a mut [f32],
    attack_flash: &'a mut [f32],
    pending_hits: &'a mut [PendingHit],
}

#[derive(Default)]
struct KernelStats {
    neighbor_checks: u64,
    full_detail_units: u32,
    strategic_units: u32,
}

impl KernelStats {
    fn include(&mut self, other: Self) {
        self.neighbor_checks += other.neighbor_checks;
        self.full_detail_units += other.full_detail_units;
        self.strategic_units += other.strategic_units;
    }
}

/// Stripe small chunks across workers: contiguous army IDs place active front
/// ranks near the start of each army, so one large block per worker would leave
/// most threads idle while two threads computed nearly all individual combat.
fn evaluate_parallel(
    context: &UnitKernel<'_>,
    batch: UnitBatch<'_>,
    workers: usize,
) -> KernelStats {
    const CHUNK_SIZE: usize = 4_096;
    let UnitBatch {
        start,
        next_positions,
        strategic_velocities,
        cooldowns,
        headings,
        attack_flash,
        pending_hits,
    } = batch;
    let mut jobs: Vec<Vec<UnitBatch<'_>>> = (0..workers).map(|_| Vec::new()).collect();
    for (index, (((((positions, strategic), cooldowns), headings), flashes), hits)) in
        next_positions
            .chunks_mut(CHUNK_SIZE)
            .zip(strategic_velocities.chunks_mut(CHUNK_SIZE))
            .zip(cooldowns.chunks_mut(CHUNK_SIZE))
            .zip(headings.chunks_mut(CHUNK_SIZE))
            .zip(attack_flash.chunks_mut(CHUNK_SIZE))
            .zip(pending_hits.chunks_mut(CHUNK_SIZE))
            .enumerate()
    {
        jobs[index % workers].push(UnitBatch {
            start: start + index * CHUNK_SIZE,
            next_positions: positions,
            strategic_velocities: strategic,
            cooldowns,
            headings,
            attack_flash: flashes,
            pending_hits: hits,
        });
    }
    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .into_iter()
            .map(|job| {
                scope.spawn(move || {
                    let mut stats = KernelStats::default();
                    for batch in job {
                        stats.include(evaluate_batch(context, batch));
                    }
                    stats
                })
            })
            .collect();
        let mut stats = KernelStats::default();
        for handle in handles {
            stats.include(handle.join().expect("unit worker panicked"));
        }
        stats
    })
}

fn evaluate_batch(context: &UnitKernel<'_>, batch: UnitBatch<'_>) -> KernelStats {
    let UnitBatch {
        start,
        next_positions,
        strategic_velocities,
        cooldowns,
        headings,
        attack_flash,
        pending_hits,
    } = batch;
    let units = &context.units;
    let mut stats = KernelStats::default();
    for local in 0..next_positions.len() {
        let i = start + local;
        pending_hits[local] = PendingHit::NONE;
        attack_flash[local] = (attack_flash[local] - SIM_DT * 3.5).max(0.0);
        next_positions[local] = units.positions[i];
        if !units.alive[i] {
            continue;
        }
        let position = units.positions[i];
        let team = units.teams[i];
        let category = units.categories[i];
        let definition = category.definition(units.kinds[i]);
        cooldowns[local] = (cooldowns[local] - SIM_DT).max(0.0);
        let salt = i.wrapping_mul(73).wrapping_add(context.tick as usize * 17);
        let mut target = None;
        let mut closest = (definition.range + 2.5).powi(2);
        let enemy = context.armies[team.opponent().index()];
        if context.activity_grid.is_some_and(|activity| {
            let radius = definition.range + 4.0;
            enemy.distance_squared(position) > radius * radius
                || !activity.enemy_near(position, team.opponent(), radius)
        }) {
            stats.strategic_units += 1;
            if definition.speed == 0.0 {
                continue;
            }
            // Stable ID phases spread strategic steering over eight ticks.
            // Every real position still integrates on every 50 ms tick.
            if context.tick % 8 == i as u64 % 8
                || strategic_velocities[local].length_squared() < 0.001
            {
                // Preserve broad lanes while marching. A centroid-only
                // goal would squeeze a million rear units into one point.
                let goal = enemy.march_goal(position);
                let forward = if category.ignores_ground_obstacles() {
                    (goal - position).normalized_or_zero()
                } else {
                    route_direction(context.map, position, goal)
                };
                let terrain_speed = if category.ignores_ground_obstacles() {
                    1.0
                } else {
                    speed_factor(context.map, position)
                };
                strategic_velocities[local] = forward * (definition.speed * terrain_speed);
                headings[local] = forward.y.atan2(forward.x);
            }
            let velocity = strategic_velocities[local];
            let ahead = position + velocity * 0.4;
            let density = context
                .grid
                .occupancy(position)
                .max(context.grid.occupancy(ahead));
            let mobility = 1.0 / (1.0 + (density as f32 - 14.0).max(0.0) * 0.18);
            let proposed = position + velocity * (SIM_DT * mobility);
            next_positions[local] = constrain_move(
                if category.ignores_ground_obstacles() {
                    crate::MapKind::Meadow
                } else {
                    context.map
                },
                position,
                proposed,
                context.extent,
            );
            continue;
        }
        stats.full_detail_units += 1;
        // Conservatively skip an empty local target query for rear ranks.
        // The AABB encloses every living opponent, so this changes no hit.
        if enemy.distance_squared(position) <= closest {
            stats.neighbor_checks += context.grid.visit_nearby(
                position,
                definition.range + 2.5,
                team.opponent(),
                64,
                salt,
                |other| {
                    let height = category.elevation() - units.categories[other].elevation();
                    let distance =
                        position.distance_squared(units.positions[other]) + height * height;
                    if distance < closest
                        || (distance == closest && target.is_none_or(|old| other < old))
                    {
                        closest = distance;
                        target = Some(other);
                    }
                },
            ) as u64;
        }

        let in_range = target.is_some() && closest <= definition.range * definition.range;
        if in_range && cooldowns[local] <= 0.0 {
            let target = target.expect("in_range requires a target");
            let roll = random(context.seed, i as u64, context.tick + 100);
            pending_hits[local] = PendingHit {
                target: target as u32,
                damage: definition.damage * (0.88 + roll * 0.24),
            };
            cooldowns[local] = definition.attack_period;
            attack_flash[local] = 1.0;
        }

        let fallback_goal = if context.activity_grid.is_some() {
            enemy.march_goal(position)
        } else {
            enemy.center
        };
        let goal = target.map_or(fallback_goal, |other| units.positions[other]);
        let forward = if category.ignores_ground_obstacles() {
            (goal - position).normalized_or_zero()
        } else {
            route_direction(context.map, position, goal)
        };
        let density = context
            .grid
            .occupancy(position)
            .max(context.grid.occupancy(position + forward * 1.4));
        // A macroscopic density signal supplements the bounded local
        // candidates. Crowds queue at a bridge instead of continuously
        // driving thousands of bodies into one overloaded bucket.
        let mobility = 1.0 / (1.0 + (density as f32 - 14.0).max(0.0) * 0.18);
        let desired = if in_range {
            Point::ZERO
        } else {
            let terrain_speed = if category.ignores_ground_obstacles() {
                1.0
            } else {
                speed_factor(context.map, position)
            };
            forward * (definition.speed * terrain_speed * mobility)
        };
        let mut forward_limit = desired.length();
        let mut separation = Point::ZERO;
        for other_team in Team::ALL {
            stats.neighbor_checks +=
                context
                    .grid
                    .visit_nearby(position, 2.0, other_team, 24, salt, |other| {
                        if other == i
                            || (category.elevation() - units.categories[other].elevation()).abs()
                                > 1.0
                        {
                            return;
                        }
                        let delta = position - units.positions[other];
                        let distance_squared = delta.length_squared();
                        let minimum = definition.radius
                            + units.categories[other]
                                .definition(units.kinds[other])
                                .radius
                            + 0.10;
                        let ahead = (delta * -1.0).dot(forward);
                        let lateral_squared = (distance_squared - ahead * ahead).max(0.0);
                        if ahead > 0.0 && ahead < 1.8 && lateral_squared < minimum * minimum * 0.72
                        {
                            let neighbor_forward_speed = units.velocities[other].dot(forward);
                            let clearance_speed =
                                ((ahead - minimum) / SIM_DT + neighbor_forward_speed).max(0.0);
                            forward_limit = forward_limit.min(clearance_speed);
                        }
                        if distance_squared < minimum * minimum && distance_squared > 0.000001 {
                            let distance = distance_squared.sqrt();
                            separation += delta * ((minimum - distance) / distance * 5.0);
                        } else if distance_squared <= 0.000001 {
                            // Stable tie-break, never divide by zero for imported
                            // or manually adjusted overlapping states.
                            separation.x += if i < other { -0.25 } else { 0.25 };
                        }
                    }) as u64;
        }
        let mut velocity =
            (desired + separation.clamp_length(2.4)).clamp_length(definition.speed * 1.25);
        let forward_speed = velocity.dot(forward);
        if forward_speed > forward_limit {
            velocity = velocity - forward * (forward_speed - forward_limit);
        }
        let proposed = position + velocity * SIM_DT;
        next_positions[local] = if definition.speed == 0.0 {
            position
        } else {
            constrain_move(
                if category.ignores_ground_obstacles() {
                    crate::MapKind::Meadow
                } else {
                    context.map
                },
                position,
                proposed,
                context.extent,
            )
        };
        if in_range {
            let facing = goal - position;
            headings[local] = facing.y.atan2(facing.x);
        } else if velocity.length_squared() > 0.02 {
            headings[local] = velocity.y.atan2(velocity.x);
        }
    }

    stats
}

fn formation_slot(index: u32, count: u32, formation: Formation) -> (f32, f32) {
    match formation {
        Formation::Wedge => {
            // Row r begins at r² and contains 2r+1 distinct positions.
            let row = (index as f64).sqrt().floor() as u32;
            let column = index - row * row;
            let row_count = (2 * row + 1).min(count - row * row);
            (row as f32, column as f32 - (row_count - 1) as f32 * 0.5)
        }
        Formation::Lines | Formation::Block => {
            let aspect = if formation == Formation::Lines {
                1.8
            } else {
                1.0
            };
            let columns = ((count as f32).sqrt() * aspect).ceil() as u32;
            let columns = columns.max(1).min(count);
            let row = index / columns;
            let column = index % columns;
            (row as f32, column as f32 - (columns - 1) as f32 * 0.5)
        }
    }
}

/// Counter-derived SplitMix64 output: no global RNG state or thread-order input.
fn random(seed: u64, unit: u64, counter: u64) -> f32 {
    let mut x = seed
        .wrapping_add(unit.wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .wrapping_add(counter.wrapping_mul(0xbf58_476d_1ce4_e5b9));
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x >> 40) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_count_preserves_complete_state_and_simultaneous_damage() {
        let mut config = ScenarioConfig::default();
        config.blue.count = 12_000;
        config.red.count = 12_000;
        let mut serial = Battle::new(config.clone()).unwrap();
        let mut parallel = Battle::new(config).unwrap();
        serial.worker_count = 1;
        parallel.worker_count = 4;
        for battle in [&mut serial, &mut parallel] {
            // Attackers from several separate worker chunks share nearby
            // targets. Their floating-point damage sum must retain ID order.
            for i in [0, 4_096, 8_192, 12_000, 16_096, 20_192] {
                for offset in 0..32 {
                    let unit = i + offset;
                    let side = if unit < 12_000 { -0.4 } else { 0.4 };
                    battle.units.positions[unit] = Point::new(side, offset as f32 * 0.02);
                    battle.units.health[unit] = 35.0;
                    battle.cooldowns[unit] = 0.0;
                }
            }
        }
        for _ in 0..12 {
            serial.step();
            parallel.step();
            assert_eq!(serial.units.positions, parallel.units.positions);
            assert_eq!(
                serial.units.previous_positions,
                parallel.units.previous_positions
            );
            assert_eq!(serial.units.velocities, parallel.units.velocities);
            assert_eq!(serial.units.headings, parallel.units.headings);
            assert_eq!(serial.units.attack_flash, parallel.units.attack_flash);
            assert_eq!(serial.units.health, parallel.units.health);
            assert_eq!(serial.units.alive, parallel.units.alive);
            assert_eq!(serial.cooldowns, parallel.cooldowns);
            assert_eq!(serial.strategic_velocities, parallel.strategic_velocities);
            assert_eq!(serial.stats.alive, parallel.stats.alive);
            assert_eq!(serial.stats.casualties, parallel.stats.casualties);
            assert_eq!(serial.stats.neighbor_checks, parallel.stats.neighbor_checks);
            assert_eq!(
                serial.stats.full_detail_units,
                parallel.stats.full_detail_units
            );
            assert_eq!(serial.stats.strategic_units, parallel.stats.strategic_units);
            assert_eq!(serial.outcome, parallel.outcome);
        }
        assert!(parallel.stats.casualties.iter().sum::<u32>() > 0);
    }
}
