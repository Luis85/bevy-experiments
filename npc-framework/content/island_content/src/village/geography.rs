//! Shared integer island geography. Plots describe clear land, not ownership or actions.
use super::catalog;
use bevy_ecs::prelude::*;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::Serialize;

pub const CELL_MM: i32 = 1000;
pub const MIN_CELL: i32 = -32;
pub const MAX_CELL_EXCLUSIVE: i32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Road {
    pub x: i32,
    pub z: i32,
    pub width_mm: i32,
    pub depth_mm: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ExpansionPlot<'a> {
    pub id: &'a str,
    pub label: &'a str,
    /// Rectangle center, in authoritative millimetres.
    pub x: i32,
    pub z: i32,
    pub width_mm: i32,
    pub depth_mm: i32,
    pub use_kind: &'a str,
}

/// Read-only wayfinding for existing trades, without borders or shop ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct District {
    pub id: &'static str,
    pub label: &'static str,
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct BoundarySegment {
    pub pos: SimPos,
    pub footprint: Footprint,
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct Geography {
    pub radii_mm: (i32, i32),
    pub roads: &'static [Road],
    pub expansion_plots: &'static [ExpansionPlot<'static>],
    pub districts: &'static [District],
    pub wildlife_home: SimPos,
    resources: &'static [(&'static str, SimPos)],
}

const FIXED_ROADS: &[Road] = &[
    Road {
        x: 0,
        z: 0,
        width_mm: 2000,
        depth_mm: 22000,
    },
    Road {
        x: 0,
        z: 0,
        width_mm: 31000,
        depth_mm: 2000,
    },
    Road {
        x: -6000,
        z: 3000,
        width_mm: 15000,
        depth_mm: 1300,
    },
    Road {
        x: 7000,
        z: -4000,
        width_mm: 1300,
        depth_mm: 14000,
    },
];

const LIVING_ROADS: &[Road] = &[
    Road {
        x: 0,
        z: 0,
        width_mm: 54000,
        depth_mm: 2000,
    },
    Road {
        x: 5000,
        z: 0,
        width_mm: 1800,
        depth_mm: 27000,
    },
    Road {
        x: -5000,
        z: 3000,
        width_mm: 22000,
        depth_mm: 1300,
    },
    Road {
        x: 7000,
        z: -4000,
        width_mm: 1300,
        depth_mm: 14000,
    },
    Road {
        x: 0,
        z: -12500,
        width_mm: 44000,
        depth_mm: 2000,
    },
    Road {
        x: 0,
        z: 13000,
        width_mm: 28000,
        depth_mm: 2000,
    },
    Road {
        x: -25000,
        z: -2500,
        width_mm: 1600,
        depth_mm: 7000,
    },
    Road {
        x: 26000,
        z: 4000,
        width_mm: 1600,
        depth_mm: 10000,
    },
    Road {
        x: -21000,
        z: 6000,
        width_mm: 1600,
        depth_mm: 14000,
    },
    Road {
        x: -21000,
        z: 13000,
        width_mm: 4000,
        depth_mm: 1400,
    },
    Road {
        x: -17000,
        z: 11000,
        width_mm: 8000,
        depth_mm: 1400,
    },
    Road {
        x: -16500,
        z: 14000,
        width_mm: 1600,
        depth_mm: 10000,
    },
    Road {
        x: 21000,
        z: -6000,
        width_mm: 1600,
        depth_mm: 14000,
    },
];

const EXPANSION_PLOTS: &[ExpansionPlot] = &[
    ExpansionPlot {
        id: "housing_sw_outer",
        label: "House plot 1",
        x: -15000,
        z: -16500,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "housing_sw_inner",
        label: "House plot 2",
        x: -5000,
        z: -16500,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "housing_se_inner",
        label: "House plot 3",
        x: 5000,
        z: -16500,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "housing_se_outer",
        label: "House plot 4",
        x: 15000,
        z: -16500,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "garden_nw",
        label: "Garden plot 1",
        x: -5000,
        z: 17000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "garden",
    },
    ExpansionPlot {
        id: "garden_ne",
        label: "Garden plot 2",
        x: 5000,
        z: 17000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "garden",
    },
];

pub const FIXED: Geography = Geography {
    radii_mm: (24000, 20000),
    roads: FIXED_ROADS,
    expansion_plots: &[],
    districts: &[],
    wildlife_home: SimPos::new(-13000, 12000),
    resources: &[
        (catalog::GRAIN, SimPos::new(-15500, 7500)),
        ("log", SimPos::new(-17500, -4500)),
        ("ore", SimPos::new(17500, 8500)),
        (catalog::WATER, SimPos::new(8500, 3500)),
        (catalog::FIBER, SimPos::new(-18000, 2000)),
        (catalog::BERRIES, SimPos::new(-5000, -13000)),
    ],
};

// The regional profile retains the shared homestead action contract. These
// authored clearings are independent of the original island's six plots.
const REGIONAL_PLOTS: &[ExpansionPlot] = &[
    ExpansionPlot {
        id: "harbour_home_w",
        label: "Harbour west homestead",
        x: -40000,
        z: -50000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "harbour_home_e",
        label: "Harbour east homestead",
        x: 40000,
        z: -50000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "harbour_garden_w",
        label: "Harbour kitchen garden",
        x: -10000,
        z: -65000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "garden",
    },
    ExpansionPlot {
        id: "harbour_garden_e",
        label: "Harbour orchard garden",
        x: 10000,
        z: -65000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "garden",
    },
    ExpansionPlot {
        id: "alderwood_home_w",
        label: "Alderwood woodland homestead",
        x: -96000,
        z: 14000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "alderwood_home_e",
        label: "Alderwood valley homestead",
        x: -35000,
        z: 64000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "alderwood_garden_w",
        label: "Alderwood herb garden",
        x: -76000,
        z: 78000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "garden",
    },
    ExpansionPlot {
        id: "alderwood_garden_e",
        label: "Alderwood berry garden",
        x: -58000,
        z: 78000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "garden",
    },
    ExpansionPlot {
        id: "ridge_home_w",
        label: "Stone Ridge valley homestead",
        x: 30000,
        z: 72000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "ridge_home_e",
        label: "Stone Ridge eastern homestead",
        x: 111000,
        z: 10000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "housing",
    },
    ExpansionPlot {
        id: "ridge_garden_w",
        label: "Stone Ridge terrace garden",
        x: 58000,
        z: 78000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "garden",
    },
    ExpansionPlot {
        id: "ridge_garden_e",
        label: "Stone Ridge sheltered garden",
        x: 76000,
        z: 78000,
        width_mm: 6000,
        depth_mm: 6000,
        use_kind: "garden",
    },
];
const REGIONAL: Geography = Geography {
    radii_mm: (121000, 119000),
    roads: &[],
    expansion_plots: REGIONAL_PLOTS,
    districts: &[
        District {
            id: "harbour",
            label: "Harbour",
            x: 0,
            z: -30000,
        },
        District {
            id: "alderwood",
            label: "Alderwood",
            x: -65000,
            z: 40000,
        },
        District {
            id: "stone_ridge",
            label: "Stone Ridge",
            x: 65000,
            z: 40000,
        },
    ],
    wildlife_home: SimPos::new(-93000, 68000),
    resources: &[],
};
pub(crate) fn regional_plots() -> &'static [ExpansionPlot<'static>] {
    REGIONAL_PLOTS
}

pub const LIVING: Geography = Geography {
    radii_mm: (30000, 25000),
    roads: LIVING_ROADS,
    expansion_plots: EXPANSION_PLOTS,
    districts: &[
        District {
            id: "market_food",
            label: "Market & food",
            x: 0,
            z: 4000,
        },
        District {
            id: "workshop_row",
            label: "Workshop row",
            x: -2000,
            z: -4500,
        },
        District {
            id: "north_fields",
            label: "North fields",
            x: -16500,
            z: 13000,
        },
    ],
    wildlife_home: SimPos::new(-16500, 17500),
    resources: &[
        (catalog::GRAIN, SimPos::new(-21500, 9500)),
        ("log", SimPos::new(-25500, -5500)),
        ("ore", SimPos::new(26500, 7500)),
        (catalog::WATER, SimPos::new(8500, 3500)),
        (catalog::FIBER, SimPos::new(-22500, 13500)),
        (catalog::BERRIES, SimPos::new(21500, -9500)),
    ],
};

impl Geography {
    pub fn cell_bounds(&self) -> GridBounds {
        GridBounds::centered(64, 64)
    }

    pub const fn cell_size_mm(&self) -> i32 {
        CELL_MM
    }

    /// Exact integer ellipse membership, safe even for arbitrary i32 positions.
    pub fn contains_mm(&self, pos: SimPos) -> bool {
        let x = i128::from(pos.x);
        let z = i128::from(pos.z);
        let rx = i128::from(self.radii_mm.0);
        let rz = i128::from(self.radii_mm.1);
        x * x * rz * rz + z * z * rx * rx <= rx * rx * rz * rz
    }

    /// Cells are classified at their centers, matching the incumbent Fixed mask.
    pub fn land_cell(&self, x: i32, z: i32) -> bool {
        if !(MIN_CELL..MAX_CELL_EXCLUSIVE).contains(&x)
            || !(MIN_CELL..MAX_CELL_EXCLUSIVE).contains(&z)
        {
            return false;
        }
        self.contains_mm(SimPos::new(
            x * CELL_MM + CELL_MM / 2,
            z * CELL_MM + CELL_MM / 2,
        ))
    }

    pub fn resource_position(&self, kind: &str) -> Option<SimPos> {
        self.resources
            .iter()
            .find(|(name, _)| *name == kind)
            .map(|(_, pos)| *pos)
    }

    /// One full-water span or two outside-ellipse spans, ordered left then right.
    pub fn shoreline_segments(&self, z: i32) -> [Option<BoundarySegment>; 2] {
        if !(MIN_CELL..MAX_CELL_EXCLUSIVE).contains(&z) {
            return [None, None];
        }
        let lo = (MIN_CELL..MAX_CELL_EXCLUSIVE).find(|x| self.land_cell(*x, z));
        let hi = (MIN_CELL..MAX_CELL_EXCLUSIVE)
            .rev()
            .find(|x| self.land_cell(*x, z));
        let spans = match (lo, hi) {
            (Some(lo), Some(hi)) => [(MIN_CELL, lo - 1), (hi + 1, MAX_CELL_EXCLUSIVE - 1)],
            _ => [
                (MIN_CELL, MAX_CELL_EXCLUSIVE - 1),
                (MAX_CELL_EXCLUSIVE, MAX_CELL_EXCLUSIVE - 1),
            ],
        };
        spans.map(|(a, b)| {
            (a <= b).then_some(BoundarySegment {
                pos: SimPos::new((a + b + 1) * CELL_MM / 2, z * CELL_MM + CELL_MM / 2),
                footprint: Footprint {
                    half_x_mm: (b - a) * CELL_MM / 2,
                    half_z_mm: 0,
                },
            })
        })
    }
}

/// Immutable construction data are rebuilt by the same saved scenario factory.
/// Registering a Living-only name prevents earlier shoreline saves loading as
/// though they already contained the expanded walkable region.
impl SimHashResource for Geography {
    const NAME: &'static str = "island_living_geography_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(CELL_MM);
        h.i32(MIN_CELL);
        h.i32(MAX_CELL_EXCLUSIVE);
        h.i32(self.radii_mm.0);
        h.i32(self.radii_mm.1);
        h.u32(self.roads.len() as u32);
        for road in self.roads {
            h.i32(road.x);
            h.i32(road.z);
            h.i32(road.width_mm);
            h.i32(road.depth_mm);
        }
        h.u32(self.expansion_plots.len() as u32);
        for plot in self.expansion_plots {
            h.str(plot.id);
            h.str(plot.label);
            h.i32(plot.x);
            h.i32(plot.z);
            h.i32(plot.width_mm);
            h.i32(plot.depth_mm);
            h.str(plot.use_kind);
        }
        h.u32(self.districts.len() as u32);
        for district in self.districts {
            h.str(district.id);
            h.str(district.label);
            h.i32(district.x);
            h.i32(district.z);
        }
        self.wildlife_home.sim_hash(h);
        h.u32(self.resources.len() as u32);
        for (kind, pos) in self.resources {
            h.str(kind);
            pos.sim_hash(h);
        }
    }
}

pub fn for_world(_world: &World) -> &'static Geography {
    &REGIONAL
}

/// The centre of a one-metre cell, saturating at the edges of `i32` so a
/// cell read from a save or a command can never overflow.
pub fn cell_centre(x: i32, z: i32) -> npc_framework::prelude::SimPos {
    npc_framework::prelude::SimPos::new(
        x.saturating_mul(1000).saturating_add(500),
        z.saturating_mul(1000).saturating_add(500),
    )
}
