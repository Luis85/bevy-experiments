use crate::{Point, Team, UnitStore};

/// Team-separated cell ranges avoid spending the target-search budget on
/// friendly units. All buffers retain capacity throughout the battle.
pub(crate) struct SpatialGrid {
    dimension: usize,
    extent: f32,
    cell_size: f32,
    counts: Vec<u32>,
    offsets: Vec<u32>,
    cursor: Vec<u32>,
    entries: Vec<u32>,
    pub max_occupancy: usize,
}

impl SpatialGrid {
    pub fn new(extent: f32, units: usize) -> Self {
        let cell_size = 3.0;
        let dimension = ((extent * 2.0) / cell_size).ceil() as usize + 1;
        let buckets = dimension * dimension * 2;
        Self {
            dimension,
            extent,
            cell_size,
            counts: vec![0; buckets],
            offsets: vec![0; buckets + 1],
            cursor: vec![0; buckets],
            entries: vec![0; units],
            max_occupancy: 0,
        }
    }

    fn coordinates(&self, point: Point) -> (i32, i32) {
        let last = self.dimension as i32 - 1;
        (
            (((point.x + self.extent) / self.cell_size) as i32).clamp(0, last),
            (((point.y + self.extent) / self.cell_size) as i32).clamp(0, last),
        )
    }

    fn bucket(&self, x: i32, y: i32, team: Team) -> usize {
        (y as usize * self.dimension + x as usize) * 2 + team.index()
    }

    pub fn occupancy(&self, point: Point) -> usize {
        let (x, y) = self.coordinates(point);
        let first = self.bucket(x, y, Team::Blue);
        (self.counts[first] + self.counts[first + 1]) as usize
    }

    pub fn rebuild(&mut self, units: &UnitStore) {
        self.counts.fill(0);
        for i in 0..units.len() {
            if units.alive[i] {
                let (x, y) = self.coordinates(units.positions[i]);
                let bucket = self.bucket(x, y, units.teams[i]);
                self.counts[bucket] += 1;
            }
        }
        self.offsets[0] = 0;
        self.max_occupancy = 0;
        for i in 0..self.counts.len() {
            self.offsets[i + 1] = self.offsets[i] + self.counts[i];
            self.cursor[i] = self.offsets[i];
            if i % 2 == 0 {
                self.max_occupancy = self
                    .max_occupancy
                    .max((self.counts[i] + self.counts[i + 1]) as usize);
            }
        }
        for i in 0..units.len() {
            if units.alive[i] {
                let (x, y) = self.coordinates(units.positions[i]);
                let bucket = self.bucket(x, y, units.teams[i]);
                self.entries[self.cursor[bucket] as usize] = i as u32;
                self.cursor[bucket] += 1;
            }
        }
    }

    /// Search center-out with an explicit candidate bound. A crowded bucket is
    /// sampled from a tick/ID-dependent offset; it is never scanned in full just
    /// to select a few neighbors. This is an intentional crowd approximation.
    pub fn visit_nearby(
        &self,
        point: Point,
        radius: f32,
        team: Team,
        budget: usize,
        salt: usize,
        mut visit: impl FnMut(usize),
    ) -> usize {
        let (cx, cy) = self.coordinates(point);
        let radius_cells = (radius / self.cell_size).ceil() as i32;
        let dimension = self.dimension as i32;
        let mut checked = 0;
        for ring in 0..=radius_cells {
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    if ring != 0 && dx.abs() != ring && dy.abs() != ring {
                        continue;
                    }
                    let (x, y) = (cx + dx, cy + dy);
                    if x < 0 || y < 0 || x >= dimension || y >= dimension {
                        continue;
                    }
                    let bucket = self.bucket(x, y, team);
                    let count = self.counts[bucket] as usize;
                    if count == 0 {
                        continue;
                    }
                    let take = count.min(16).min(budget - checked);
                    let start = salt.wrapping_add(bucket) % count;
                    for n in 0..take {
                        let entry = self.offsets[bucket] as usize + (start + n) % count;
                        visit(self.entries[entry] as usize);
                    }
                    checked += take;
                    if checked >= budget {
                        return checked;
                    }
                }
            }
        }
        checked
    }

    pub fn allocated_bytes(&self) -> usize {
        (self.counts.capacity()
            + self.offsets.capacity()
            + self.cursor.capacity()
            + self.entries.capacity())
            * size_of::<u32>()
    }
}

/// Team occupancy summed-area table. It answers conservative enemy proximity
/// queries in constant time, independent of the number of units in a cell.
/// This controls simulation activity, never visibility or camera position.
pub(crate) struct ActivityGrid {
    dimension: usize,
    extent: f32,
    counts: Vec<[u32; 2]>,
    sums: Vec<[u32; 2]>,
}

impl ActivityGrid {
    const CELL_SIZE: f32 = 16.0;

    pub fn new(extent: f32) -> Self {
        let dimension = (extent * 2.0 / Self::CELL_SIZE).ceil() as usize + 1;
        Self {
            dimension,
            extent,
            counts: vec![[0; 2]; dimension * dimension],
            sums: vec![[0; 2]; (dimension + 1) * (dimension + 1)],
        }
    }

    fn coordinates(&self, point: Point) -> (usize, usize) {
        let last = self.dimension - 1;
        (
            (((point.x + self.extent) / Self::CELL_SIZE).max(0.0) as usize).min(last),
            (((point.y + self.extent) / Self::CELL_SIZE).max(0.0) as usize).min(last),
        )
    }

    pub fn rebuild(&mut self, units: &UnitStore) {
        self.counts.fill([0; 2]);
        for i in 0..units.len() {
            if units.alive[i] {
                let (x, y) = self.coordinates(units.positions[i]);
                self.counts[y * self.dimension + x][units.teams[i].index()] += 1;
            }
        }
        let stride = self.dimension + 1;
        for y in 0..self.dimension {
            let mut row = [0_u32; 2];
            for x in 0..self.dimension {
                let count = self.counts[y * self.dimension + x];
                for team in 0..2 {
                    row[team] += count[team];
                    self.sums[(y + 1) * stride + x + 1][team] =
                        self.sums[y * stride + x + 1][team] + row[team];
                }
            }
        }
    }

    pub fn enemy_near(&self, point: Point, enemy: Team, radius: f32) -> bool {
        let (left, top) = self.coordinates(Point::new(point.x - radius, point.y - radius));
        let (right, bottom) = self.coordinates(Point::new(point.x + radius, point.y + radius));
        let stride = self.dimension + 1;
        let team = enemy.index();
        let sum = i64::from(self.sums[(bottom + 1) * stride + right + 1][team])
            + i64::from(self.sums[top * stride + left][team])
            - i64::from(self.sums[top * stride + right + 1][team])
            - i64::from(self.sums[(bottom + 1) * stride + left][team]);
        sum != 0
    }

    pub fn allocated_bytes(&self) -> usize {
        (self.counts.capacity() + self.sums.capacity()) * size_of::<[u32; 2]>()
    }
}
