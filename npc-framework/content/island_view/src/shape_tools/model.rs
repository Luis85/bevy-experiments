//! Pure Shape-mode planning: a brush dab becomes absolute per-cell targets,
//! the targets become one recorded command (a `Terraform` disc when the whole
//! brush ends at one value, otherwise one `TerraformCells` batch), and a
//! bounded stroke history produces the inverse batch for undo / redo.
//!
//! Nothing here reads a `World`. Heights are sampled through a closure, so
//! relative tools (Raise, Lower, Smooth) are resolved view-side into the same
//! absolute commands the simulation already validates and records.
use island_content::village::terrain::TerrainSurface;
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Raise,
    Lower,
    Flatten,
    Smooth,
    Paint,
}

impl Tool {
    pub(crate) const ALL: [Self; 5] = [
        Self::Raise,
        Self::Lower,
        Self::Flatten,
        Self::Smooth,
        Self::Paint,
    ];
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Raise => "Raise",
            Self::Lower => "Lower",
            Self::Flatten => "Flatten",
            Self::Smooth => "Smooth",
            Self::Paint => "Paint",
        }
    }
    pub(crate) fn help(self) -> &'static str {
        match self {
            Self::Raise => "Hold and drag to lift the ground from its current height.",
            Self::Lower => "Hold and drag to sink the ground from its current height.",
            Self::Flatten => "Drag to level the ground to the height first clicked.",
            Self::Smooth => "Drag to ease each cell toward its neighbours.",
            Self::Paint => "Drag to lay the chosen surface; heights are kept.",
        }
    }
    /// Holding still keeps working for tools that accumulate.
    pub(crate) fn repeats(self) -> bool {
        matches!(self, Self::Raise | Self::Lower | Self::Smooth)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Falloff {
    /// Every cell of the brush gets the full effect.
    Hard,
    /// Full effect at the centre easing to a quarter at the rim.
    Soft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) height_mm: i32,
    pub(crate) surface: TerrainSurface,
}

/// The authoritative command's ranges; edits outside them would be refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    pub(crate) dry: (i32, i32),
    pub(crate) water: (i32, i32),
    pub(crate) max_radius: u32,
    /// Most cells one `TerraformCells` batch may carry; zero where the
    /// profile has no such command and uneven edits go cell by cell.
    pub(crate) max_batch: usize,
}

impl Limits {
    pub(crate) const REGIONAL: Self = Self {
        dry: (0, 12_000),
        water: (-2_000, 0),
        max_radius: 8,
        max_batch: island_content::village::frontier::terrain::MAX_TERRAFORM_CELLS,
    };
    /// The same ranges with every uneven edit sent cell by cell.
    pub(crate) fn per_cell(self) -> Self {
        Self {
            max_batch: 0,
            ..self
        }
    }
    fn clamp(&self, height_mm: i32, surface: TerrainSurface) -> i32 {
        let (low, high) = if surface == TerrainSurface::Water {
            self.water
        } else {
            self.dry
        };
        height_mm.clamp(low, high)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Brush {
    pub(crate) tool: Tool,
    pub(crate) radius: u32,
    pub(crate) falloff: Falloff,
    /// Surface laid by [`Tool::Paint`].
    pub(crate) surface: TerrainSurface,
    /// Height change of one full-strength Raise / Lower dab.
    pub(crate) step_mm: i32,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            tool: Tool::Raise,
            radius: 2,
            // A hard edge on level ground is a single recorded disc.
            falloff: Falloff::Hard,
            surface: TerrainSurface::Grass,
            step_mm: 250,
        }
    }
}

/// One cell's change within a dab or stroke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) x: i32,
    pub(crate) z: i32,
    pub(crate) before: Cell,
    pub(crate) after: Cell,
}

/// One recorded terrain command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// The recorded `Terraform`: every cell of a disc gets one value.
    Disc {
        x: i32,
        z: i32,
        radius: u32,
        height_mm: i32,
        surface: TerrainSurface,
    },
    /// The recorded `TerraformCells`: each listed cell gets its own value,
    /// and the simulation applies all of them or none.
    Cells(Vec<((i32, i32), Cell)>),
}

impl Command {
    /// The cells the command addresses.
    pub(crate) fn footprint(&self) -> Vec<(i32, i32)> {
        match self {
            Self::Disc { x, z, radius, .. } => disc((*x, *z), *radius).collect(),
            Self::Cells(cells) => cells.iter().map(|(at, _)| *at).collect(),
        }
    }
}

/// The same circular footprint the command applies, clipped by `sample`.
pub(crate) fn disc(center: (i32, i32), radius: u32) -> impl Iterator<Item = (i32, i32)> {
    let r = radius as i32;
    (-r..=r)
        .flat_map(move |dz| (-r..=r).map(move |dx| (dx, dz)))
        .filter(move |(dx, dz)| dx * dx + dz * dz <= r * r)
        .map(move |(dx, dz)| (center.0.saturating_add(dx), center.1.saturating_add(dz)))
}

fn isqrt(value: i32) -> i32 {
    let mut root = 0;
    while (root + 1) * (root + 1) <= value {
        root += 1;
    }
    root
}

/// Effect strength in permille for a cell `(dx, dz)` from the brush centre.
pub(crate) fn weight(falloff: Falloff, radius: u32, dx: i32, dz: i32) -> i32 {
    match falloff {
        Falloff::Hard => 1000,
        Falloff::Soft if radius == 0 => 1000,
        Falloff::Soft => {
            // Distance in milli-cells keeps the ramp smooth on small brushes.
            let distance = isqrt((dx * dx + dz * dz) * 10_000).min(radius as i32 * 100);
            1000 - 750 * distance / (radius as i32 * 100)
        }
    }
}

fn scaled(delta: i32, weight: i32) -> i32 {
    // Round half away from zero so a soft rim still moves by at least 1 mm.
    let product = i64::from(delta) * i64::from(weight);
    ((product + product.signum() * 500) / 1000) as i32
}

/// Plan one dab at `center`. `anchor_mm` is the Flatten target (the height
/// under the cursor when the stroke began). Returns only cells that change.
pub(crate) fn plan_dab(
    brush: &Brush,
    center: (i32, i32),
    anchor_mm: i32,
    limits: &Limits,
    sample: impl Fn(i32, i32) -> Option<Cell>,
) -> Vec<Edit> {
    let radius = brush.radius.min(limits.max_radius);
    let mut edits = Vec::new();
    for (x, z) in disc(center, radius) {
        let Some(before) = sample(x, z) else { continue };
        let weight = weight(brush.falloff, radius, x - center.0, z - center.1);
        let mut after = before;
        match brush.tool {
            Tool::Raise => after.height_mm = before.height_mm + scaled(brush.step_mm, weight),
            Tool::Lower => after.height_mm = before.height_mm - scaled(brush.step_mm, weight),
            Tool::Flatten => {
                after.height_mm = before.height_mm + scaled(anchor_mm - before.height_mm, weight);
            }
            Tool::Smooth => {
                // Average the 3x3 neighbourhood of the same kind of ground, so
                // a shoreline is never pulled across sea level.
                let water = before.surface == TerrainSurface::Water;
                let (mut sum, mut count) = (0_i64, 0_i64);
                for nz in z - 1..=z + 1 {
                    for nx in x - 1..=x + 1 {
                        if let Some(near) = sample(nx, nz)
                            && (near.surface == TerrainSurface::Water) == water
                        {
                            sum += i64::from(near.height_mm);
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    let average = (sum / count) as i32;
                    after.height_mm = before.height_mm + scaled(average - before.height_mm, weight);
                }
            }
            Tool::Paint => after.surface = brush.surface,
        }
        after.height_mm = limits.clamp(after.height_mm, after.surface);
        if after != before {
            edits.push(Edit {
                x,
                z,
                before,
                after,
            });
        }
    }
    edits
}

/// Turn a dab's edits into recorded commands: nothing for an idle dab, one
/// disc when every cell of the brush ends at one value, otherwise one batch
/// of the changed cells. Only a profile without the batch command
/// (`max_batch == 0`) falls back to one radius-0 disc per changed cell.
pub(crate) fn batch(
    edits: &[Edit],
    center: (i32, i32),
    radius: u32,
    max_batch: usize,
    sample: impl Fn(i32, i32) -> Option<Cell>,
) -> Vec<Command> {
    let Some(first) = edits.first() else {
        return Vec::new();
    };
    let target = first.after;
    let planned: BTreeMap<(i32, i32), Cell> = edits
        .iter()
        .map(|edit| ((edit.x, edit.z), edit.after))
        .collect();
    let uniform = sample(center.0, center.1).is_some()
        && disc(center, radius).all(|(x, z)| {
            match planned.get(&(x, z)).copied().or_else(|| sample(x, z)) {
                Some(cell) => cell == target,
                // Outside the map the command clips exactly like the plan.
                None => true,
            }
        });
    if uniform {
        return vec![Command::Disc {
            x: center.0,
            z: center.1,
            radius,
            height_mm: target.height_mm,
            surface: target.surface,
        }];
    }
    cell_commands(
        edits.iter().map(|edit| ((edit.x, edit.z), edit.after)),
        max_batch,
    )
}

/// Per-cell values as recorded commands: batches of at most `max_batch`
/// cells (one, for anything a brush or a bounded stroke can produce), or one
/// radius-0 disc per cell where the profile has no batch command.
fn cell_commands(
    cells: impl Iterator<Item = ((i32, i32), Cell)>,
    max_batch: usize,
) -> Vec<Command> {
    let cells: Vec<_> = cells.collect();
    if max_batch == 0 {
        return cells
            .into_iter()
            .map(|((x, z), cell)| Command::Disc {
                x,
                z,
                radius: 0,
                height_mm: cell.height_mm,
                surface: cell.surface,
            })
            .collect();
    }
    cells
        .chunks(max_batch)
        .map(|chunk| Command::Cells(chunk.to_vec()))
        .collect()
}

/// Everything one press-drag-release changed: first `before`, latest `after`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stroke {
    cells: BTreeMap<(i32, i32), (Cell, Cell)>,
}

impl Stroke {
    pub(crate) fn absorb(&mut self, edits: &[Edit]) {
        for edit in edits {
            self.cells
                .entry((edit.x, edit.z))
                .and_modify(|(_, after)| *after = edit.after)
                .or_insert((edit.before, edit.after));
        }
    }
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.cells.len()
    }
    /// How many cells the stroke would cover after absorbing `edits`.
    pub(crate) fn len_with(&self, edits: &[Edit]) -> usize {
        self.cells.len()
            + edits
                .iter()
                .filter(|edit| !self.cells.contains_key(&(edit.x, edit.z)))
                .count()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
    /// The simulation refused a dab: take its edits back out. A cell whose
    /// latest value is still the refused one returns to what it held before
    /// that dab; a cell a later dab has written since is left alone. Cells
    /// that end where they began leave the stroke. Returns whether anything
    /// of the dab was in this stroke.
    pub(crate) fn retract(&mut self, edits: &[Edit]) -> bool {
        let mut found = false;
        for edit in edits {
            let at = (edit.x, edit.z);
            let Some((before, after)) = self.cells.get_mut(&at) else {
                continue;
            };
            if *after != edit.after {
                continue;
            }
            found = true;
            *after = edit.before;
            if *before == *after {
                self.cells.remove(&at);
            }
        }
        found
    }
    fn commands(
        &self,
        restore_before: bool,
        max_batch: usize,
        sample: impl Fn(i32, i32) -> Option<Cell>,
    ) -> Vec<Command> {
        cell_commands(
            self.cells.iter().filter_map(|(at, (before, after))| {
                let wanted = if restore_before { *before } else { *after };
                // A cell that already holds the value (a refused edit, or a
                // later identical one) needs no command.
                (sample(at.0, at.1) != Some(wanted)).then_some((*at, wanted))
            }),
            max_batch,
        )
    }
    /// The cell values this stroke would leave after an undo or redo.
    fn expectations(&self, restore_before: bool) -> Vec<((i32, i32), Cell)> {
        self.cells
            .iter()
            .map(|(at, (before, after))| (*at, if restore_before { *before } else { *after }))
            .collect()
    }
}

/// Which way a history step went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Undo,
    Redo,
}

/// A replayed history step: the commands to record and the values to expect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Replay {
    /// The stroke replayed, so a refused step can be taken back.
    pub(crate) stroke: u64,
    pub(crate) commands: Vec<Command>,
    pub(crate) expect: Vec<((i32, i32), Cell)>,
}

/// Bounded undo / redo. It stores view-side cell values only; the world is
/// changed exclusively by the commands it hands back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct History {
    undo: VecDeque<(u64, Stroke)>,
    redo: Vec<(u64, Stroke)>,
    limit: usize,
    next: u64,
}

impl History {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            next: 1,
        }
    }
    pub(crate) fn push(&mut self, stroke: Stroke) {
        if stroke.is_empty() {
            return;
        }
        self.redo.clear();
        self.undo.push_back((self.next, stroke));
        self.next += 1;
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
    }
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// How many strokes can be taken back, and how many the history keeps.
    pub(crate) fn depth(&self) -> (usize, usize) {
        (self.undo.len(), self.limit)
    }
    /// The identity of the newest stroke ever recorded (0 before the
    /// first): placements are stamped with it to share one timeline.
    pub(crate) fn issued(&self) -> u64 {
        self.next - 1
    }
    /// The stroke Undo would take back next.
    pub(crate) fn newest_undo(&self) -> Option<u64> {
        self.undo.back().map(|(id, _)| *id)
    }
    /// The stroke Redo would lay again next.
    pub(crate) fn next_redo(&self) -> Option<u64> {
        self.redo.last().map(|(id, _)| *id)
    }
    /// Something new was done (a placement): the strokes taken back cannot
    /// be laid again.
    pub(crate) fn clear_redo(&mut self) {
        self.redo.clear();
    }
    /// The commands that restore the newest stroke's first captured values.
    /// With a batch command (`max_batch > 0`) that is one command.
    pub(crate) fn undo(
        &mut self,
        max_batch: usize,
        sample: impl Fn(i32, i32) -> Option<Cell>,
    ) -> Option<Replay> {
        let (id, stroke) = self.undo.pop_back()?;
        let replay = Replay {
            stroke: id,
            commands: stroke.commands(true, max_batch, sample),
            expect: stroke.expectations(true),
        };
        self.redo.push((id, stroke));
        Some(replay)
    }
    pub(crate) fn redo(
        &mut self,
        max_batch: usize,
        sample: impl Fn(i32, i32) -> Option<Cell>,
    ) -> Option<Replay> {
        let (id, stroke) = self.redo.pop()?;
        let replay = Replay {
            stroke: id,
            commands: stroke.commands(false, max_batch, sample),
            expect: stroke.expectations(false),
        };
        self.undo.push_back((id, stroke));
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
        Some(replay)
    }
    /// The simulation refused a dab of a stroke that is already in the
    /// history: take the dab's edits out of the newest stroke that holds
    /// them, and drop a stroke that is left with nothing.
    pub(crate) fn retract(&mut self, edits: &[Edit]) {
        let Some(at) = (0..self.undo.len())
            .rev()
            .find(|at| self.undo[*at].1.retract(edits))
        else {
            return;
        };
        if self.undo[at].1.is_empty() {
            self.undo.remove(at);
        }
    }
    /// The simulation refused a step's one command, so nothing changed: put
    /// the stroke back where it was. A step is only taken back while it is
    /// still the latest one; a stroke drawn since has started a new future.
    pub(crate) fn take_back(&mut self, stroke: u64, step: Step) -> bool {
        match step {
            Step::Undo if self.redo.last().is_some_and(|(id, _)| *id == stroke) => {
                let entry = self.redo.pop().expect("checked");
                self.undo.push_back(entry);
                true
            }
            Step::Redo if self.undo.back().is_some_and(|(id, _)| *id == stroke) => {
                let entry = self.undo.pop_back().expect("checked");
                self.redo.push(entry);
                true
            }
            _ => false,
        }
    }
}

// --- What the interface says ------------------------------------------------

/// The key that takes up a tool.
pub(crate) fn tool_key(tool: Tool) -> &'static str {
    match tool {
        Tool::Raise => "R",
        Tool::Lower => "F",
        Tool::Flatten => "T",
        Tool::Smooth => "G",
        Tool::Paint => "P",
    }
}

/// What the history is, for the second level of Undo's and Redo's tooltips.
pub(crate) const HISTORY_SENTENCE: &str = "Each press, drag and release is one stroke. The last 32 strokes can be taken back; a new stroke forgets what was undone.";

pub(crate) fn falloff_name(falloff: Falloff) -> &'static str {
    match falloff {
        Falloff::Hard => "Hard",
        Falloff::Soft => "Soft",
    }
}

/// The brush strengths: the height one full Raise / Lower dab changes, by
/// name. "Normal" is the default step.
pub(crate) const STRENGTHS: [(&str, i32); 3] = [("Gentle", 100), ("Normal", 250), ("Strong", 500)];

/// What a strength does, for its tooltip.
pub(crate) fn strength_help(step_mm: i32) -> String {
    let centimetres = step_mm / 10;
    format!("Raise and Lower move the ground {centimetres} cm a dab at the centre.")
}

pub(crate) fn falloff_help(falloff: Falloff) -> &'static str {
    match falloff {
        Falloff::Hard => "Every cell under the brush gets the full effect.",
        Falloff::Soft => "Full effect at the centre, easing to a quarter at the rim.",
    }
}

/// The swatch of a surface on the brush bar, as sRGB.
pub(crate) fn surface_swatch(surface: TerrainSurface) -> [f32; 3] {
    match surface {
        TerrainSurface::Grass => [0.42, 0.62, 0.30],
        TerrainSurface::Soil => [0.55, 0.40, 0.26],
        TerrainSurface::Stone => [0.60, 0.60, 0.58],
        TerrainSurface::Water => [0.24, 0.52, 0.72],
    }
}

/// The brush's reach in metres: a cell is one metre, and the brush covers
/// the cell under the cursor and `radius` cells to each side.
pub(crate) fn size_text(radius: u32) -> String {
    format!("{} m", radius * 2 + 1)
}

/// The history depth under Undo: "12 of 32".
pub(crate) fn history_text(depth: usize, kept: usize) -> String {
    format!("{depth} of {kept}")
}

/// How loud a line of the tool's own word is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Weight {
    Plain,
    Warning,
    Refusal,
}

/// The tool's word as a toast, or nothing. A successful edit says nothing:
/// the ground and the depth under Undo already show it. Edits that wait for
/// a stopped world say so for as long as they wait.
pub(crate) fn status_text(status: &str, queued: usize, stopped: bool) -> Option<(Weight, String)> {
    if queued > 0 && stopped {
        let edits = if queued == 1 {
            "1 edit waits".to_owned()
        } else {
            format!("{queued} edits wait")
        };
        return Some((
            Weight::Warning,
            format!("{edits} for the world to run. Resume to apply."),
        ));
    }
    if status.is_empty() || status.starts_with("Landscape updated") {
        return None;
    }
    // "Undo requested · 13 cells" counts cells; the player counts strokes.
    if let Some((step, _)) = status.split_once(" requested") {
        return Some((
            Weight::Plain,
            if step == "Undo" {
                "Stroke taken back.".to_owned()
            } else {
                "Stroke laid again.".to_owned()
            },
        ));
    }
    let weight = if status.contains("ot applied")
        || status.starts_with("Not placed")
        || status.starts_with("Not removed")
    {
        Weight::Refusal
    } else if status.starts_with("Nothing to")
        || status.starts_with("Placed ")
        || status.starts_with("Removed ")
    {
        Weight::Plain
    } else {
        Weight::Warning
    };
    Some((weight, status.to_owned()))
}

/// The tag beside the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tag {
    /// "Raise · ground at 6.3 m", "Paint · stone at 2.0 m", "Lower · water".
    pub(crate) line: String,
    /// The simulation's reason when it would refuse, or just refused, a dab.
    pub(crate) reason: Option<String>,
}

/// What the tag says for a tool over a cell. Heights to a tenth of a metre;
/// no cell coordinates.
pub(crate) fn tag(tool: Tool, cell: Cell, reason: Option<&str>) -> Tag {
    let ground = match cell.surface {
        TerrainSurface::Water => "water".to_owned(),
        surface => {
            let name = match surface {
                TerrainSurface::Soil => "soil",
                TerrainSurface::Stone => "stone",
                _ => "ground",
            };
            let tenths = (cell.height_mm + 50).div_euclid(100);
            format!(
                "{name} at {}.{} m",
                tenths.div_euclid(10),
                tenths.rem_euclid(10)
            )
        }
    };
    Tag {
        line: format!("{} · {ground}", tool.name()),
        reason: reason.map(|reason| {
            let mut sentence = reason.trim().to_owned();
            if let Some(first) = sentence.get(..1) {
                let upper = first.to_uppercase();
                sentence.replace_range(..1, &upper);
            }
            if !sentence.ends_with(['.', '!', '?']) {
                sentence.push('.');
            }
            sentence
        }),
    }
}

/// The dashes of one edge of a refused cell's outline, as shares of the edge.
pub(crate) const DASHES: [(f32, f32); 2] = [(0.05, 0.40), (0.60, 0.95)];

#[cfg(test)]
mod tests {
    use super::*;
    use TerrainSurface::{Grass, Stone, Water};

    #[test]
    fn the_interface_words_name_tools_sizes_depth_and_refusals() {
        assert_eq!(Tool::ALL.map(tool_key), ["R", "F", "T", "G", "P"]);
        assert_eq!(size_text(0), "1 m");
        assert_eq!(size_text(4), "9 m");
        let mut history = History::new(32);
        assert_eq!(history.depth(), (0, 32));
        let mut stroke = Stroke::default();
        stroke.absorb(&[Edit {
            x: 0,
            z: 0,
            before: Cell {
                height_mm: 0,
                surface: Grass,
            },
            after: Cell {
                height_mm: 250,
                surface: Grass,
            },
        }]);
        history.push(stroke);
        let (depth, kept) = history.depth();
        assert_eq!(history_text(depth, kept), "1 of 32");
        assert!(HISTORY_SENTENCE.contains("32 strokes"));
        // The tag: tool, the ground and its height; never a cell coordinate.
        let ground = Cell {
            height_mm: 6_349,
            surface: Grass,
        };
        assert_eq!(
            tag(Tool::Raise, ground, None),
            Tag {
                line: "Raise · ground at 6.3 m".into(),
                reason: None
            }
        );
        let stone = Cell {
            height_mm: 1_960,
            surface: Stone,
        };
        assert_eq!(tag(Tool::Paint, stone, None).line, "Paint · stone at 2.0 m");
        let sea = Cell {
            height_mm: -400,
            surface: Water,
        };
        let refused = tag(Tool::Lower, sea, Some("the brush overlaps a building"));
        assert_eq!(refused.line, "Lower · water");
        assert_eq!(
            refused.reason.as_deref(),
            Some("The brush overlaps a building.")
        );
        // The tool's word: silent after success, loud for a refusal.
        assert_eq!(status_text("", 0, false), None);
        assert_eq!(
            status_text("Landscape updated · 23 edits applied", 0, false),
            None
        );
        assert_eq!(
            status_text("Undo requested · 13 cells", 0, false),
            Some((Weight::Plain, "Stroke taken back.".into()))
        );
        assert_eq!(
            status_text("Redo requested · 13 cells", 1, false),
            Some((Weight::Plain, "Stroke laid again.".into()))
        );
        assert_eq!(
            status_text("Nothing to redo.", 0, true),
            Some((Weight::Plain, "Nothing to redo.".into()))
        );
        assert_eq!(
            status_text("Not applied: too steep", 0, false).map(|(weight, _)| weight),
            Some(Weight::Refusal)
        );
        assert_eq!(
            status_text("Undo not applied: a cliff", 0, false).map(|(weight, _)| weight),
            Some(Weight::Refusal)
        );
        assert_eq!(
            status_text("Landscape updated · 2 edits applied", 3, true),
            Some((
                Weight::Warning,
                "3 edits wait for the world to run. Resume to apply.".into()
            ))
        );
        assert!(
            status_text("", 1, true)
                .unwrap()
                .1
                .starts_with("1 edit waits")
        );
        for surface in [Grass, Stone, Water, TerrainSurface::Soil] {
            assert!(
                surface_swatch(surface)
                    .iter()
                    .all(|part| (0.0..=1.0).contains(part))
            );
        }
        assert_eq!(falloff_name(Falloff::Soft), "Soft");
        assert!(falloff_help(Falloff::Hard).contains("full effect"));
    }

    type Map = BTreeMap<(i32, i32), Cell>;

    fn flat(height_mm: i32, surface: TerrainSurface, half: i32) -> Map {
        (-half..=half)
            .flat_map(|z| (-half..=half).map(move |x| ((x, z), Cell { height_mm, surface })))
            .collect()
    }
    /// What the simulation does with each command, on a plain map.
    fn apply(map: &mut Map, commands: &[Command]) {
        for command in commands {
            match command {
                Command::Disc {
                    x,
                    z,
                    radius,
                    height_mm,
                    surface,
                } => {
                    for at in disc((*x, *z), *radius) {
                        if let Some(cell) = map.get_mut(&at) {
                            *cell = Cell {
                                height_mm: *height_mm,
                                surface: *surface,
                            };
                        }
                    }
                }
                Command::Cells(cells) => {
                    let mut seen = std::collections::BTreeSet::new();
                    for (at, value) in cells {
                        assert!(seen.insert(*at), "a batch lists a cell once");
                        *map.get_mut(at).expect("a batch names only map cells") = *value;
                    }
                }
            }
        }
    }
    const BATCH: usize = Limits::REGIONAL.max_batch;
    /// Rolling ground with a stone outcrop and a pond: nothing is uniform.
    fn rough() -> Map {
        let mut map = flat(1000, Grass, 8);
        for ((x, z), cell) in &mut map {
            cell.height_mm = 1000 + 90 * ((x * 3 + z * 5).rem_euclid(7));
        }
        map.get_mut(&(1, 1)).unwrap().surface = Stone;
        map.insert(
            (-2, 0),
            Cell {
                height_mm: -400,
                surface: Water,
            },
        );
        map
    }
    fn brush(tool: Tool, radius: u32, falloff: Falloff) -> Brush {
        Brush {
            tool,
            radius,
            falloff,
            ..Brush::default()
        }
    }

    #[test]
    fn raise_and_lower_are_relative_to_each_cells_current_height() {
        let mut map = flat(1000, Grass, 6);
        map.get_mut(&(1, 0)).unwrap().height_mm = 2000;
        let edits = plan_dab(
            &brush(Tool::Raise, 1, Falloff::Hard),
            (0, 0),
            0,
            &Limits::REGIONAL,
            |x, z| map.get(&(x, z)).copied(),
        );
        assert_eq!(edits.len(), 5, "a radius-1 disc is five cells");
        for edit in &edits {
            assert_eq!(edit.after.height_mm, edit.before.height_mm + 250);
            assert_eq!(edit.after.surface, edit.before.surface);
        }
        let lowered = plan_dab(
            &brush(Tool::Lower, 0, Falloff::Hard),
            (1, 0),
            0,
            &Limits::REGIONAL,
            |x, z| map.get(&(x, z)).copied(),
        );
        assert_eq!(lowered[0].after.height_mm, 1750);
    }

    #[test]
    fn heights_stay_inside_the_commands_ranges_and_idle_dabs_are_empty() {
        let top = flat(12_000, Stone, 3);
        let none = plan_dab(
            &brush(Tool::Raise, 2, Falloff::Hard),
            (0, 0),
            0,
            &Limits::REGIONAL,
            |x, z| top.get(&(x, z)).copied(),
        );
        assert!(none.is_empty(), "nothing to record at the ceiling");
        let sea = flat(-100, Water, 3);
        let raised = plan_dab(
            &brush(Tool::Raise, 0, Falloff::Hard),
            (0, 0),
            0,
            &Limits::REGIONAL,
            |x, z| sea.get(&(x, z)).copied(),
        );
        assert_eq!(
            raised[0].after.height_mm, 0,
            "water never rises above sea level"
        );
    }

    #[test]
    fn soft_falloff_weakens_towards_the_rim() {
        assert_eq!(weight(Falloff::Hard, 4, 4, 0), 1000);
        assert_eq!(weight(Falloff::Soft, 4, 0, 0), 1000);
        assert_eq!(weight(Falloff::Soft, 4, 4, 0), 250);
        assert!(weight(Falloff::Soft, 4, 2, 0) > weight(Falloff::Soft, 4, 3, 0));
        let map = flat(1000, Grass, 6);
        let edits = plan_dab(
            &brush(Tool::Raise, 2, Falloff::Soft),
            (0, 0),
            0,
            &Limits::REGIONAL,
            |x, z| map.get(&(x, z)).copied(),
        );
        let at = |x, z| {
            edits
                .iter()
                .find(|edit| (edit.x, edit.z) == (x, z))
                .unwrap()
                .after
                .height_mm
        };
        assert_eq!(at(0, 0), 1250);
        assert!(at(1, 0) < at(0, 0) && at(2, 0) < at(1, 0) && at(2, 0) > 1000);
    }

    #[test]
    fn flatten_levels_to_the_anchor_and_paint_keeps_heights() {
        let mut map = flat(500, Grass, 4);
        map.get_mut(&(1, 1)).unwrap().height_mm = 1500;
        let edits = plan_dab(
            &brush(Tool::Flatten, 2, Falloff::Hard),
            (0, 0),
            750,
            &Limits::REGIONAL,
            |x, z| map.get(&(x, z)).copied(),
        );
        assert_eq!(edits.len(), 13);
        assert!(edits.iter().all(|edit| edit.after.height_mm == 750));

        let paint = Brush {
            tool: Tool::Paint,
            radius: 1,
            falloff: Falloff::Soft,
            surface: Stone,
            step_mm: 250,
        };
        let painted = plan_dab(&paint, (1, 1), 0, &Limits::REGIONAL, |x, z| {
            map.get(&(x, z)).copied()
        });
        assert!(painted.iter().all(
            |edit| edit.after.surface == Stone && edit.after.height_mm == edit.before.height_mm
        ));
        let water = Brush {
            surface: Water,
            ..paint
        };
        let flooded = plan_dab(&water, (1, 1), 0, &Limits::REGIONAL, |x, z| {
            map.get(&(x, z)).copied()
        });
        assert!(flooded.iter().all(|edit| edit.after.height_mm == 0));
    }

    #[test]
    fn smooth_moves_a_spike_towards_its_neighbours_without_crossing_the_shore() {
        let mut map = flat(1000, Grass, 4);
        map.get_mut(&(0, 0)).unwrap().height_mm = 1900;
        map.insert(
            (1, 0),
            Cell {
                height_mm: -900,
                surface: Water,
            },
        );
        let edits = plan_dab(
            &brush(Tool::Smooth, 0, Falloff::Hard),
            (0, 0),
            0,
            &Limits::REGIONAL,
            |x, z| map.get(&(x, z)).copied(),
        );
        // Eight dry cells: seven at 1000 and the spike; the pond is ignored.
        assert_eq!(edits[0].after.height_mm, (7 * 1000 + 1900) / 8);
    }

    #[test]
    fn a_uniform_dab_is_one_disc_command_and_an_uneven_one_is_one_batch() {
        let mut map = flat(1000, Grass, 6);
        let hard = brush(Tool::Raise, 2, Falloff::Hard);
        let edits = plan_dab(&hard, (0, 0), 0, &Limits::REGIONAL, |x, z| {
            map.get(&(x, z)).copied()
        });
        let commands = batch(&edits, (0, 0), 2, BATCH, |x, z| map.get(&(x, z)).copied());
        assert_eq!(
            commands,
            vec![Command::Disc {
                x: 0,
                z: 0,
                radius: 2,
                height_mm: 1250,
                surface: Grass
            }]
        );
        apply(&mut map, &commands);
        assert!(disc((0, 0), 2).all(|at| map[&at].height_mm == 1250));

        // Soft falloff gives different targets: one batch of thirteen cells.
        let soft = brush(Tool::Raise, 2, Falloff::Soft);
        let edits = plan_dab(&soft, (0, 0), 0, &Limits::REGIONAL, |x, z| {
            map.get(&(x, z)).copied()
        });
        let commands = batch(&edits, (0, 0), 2, BATCH, |x, z| map.get(&(x, z)).copied());
        let [Command::Cells(cells)] = commands.as_slice() else {
            panic!("one batch, not {commands:?}");
        };
        assert_eq!(cells.len(), 13);
        assert_eq!(commands[0].footprint().len(), 13);
        let mut expected = map.clone();
        for edit in &edits {
            expected.insert((edit.x, edit.z), edit.after);
        }
        apply(&mut map, &commands);
        assert_eq!(map, expected, "the batch reproduces the plan exactly");

        // A flatten that leaves some disc cells untouched at another height
        // must not overwrite them with a disc.
        let mut ridge = flat(500, Grass, 6);
        ridge.get_mut(&(2, 0)).unwrap().surface = Stone;
        let flatten = brush(Tool::Flatten, 2, Falloff::Hard);
        let edits = plan_dab(&flatten, (0, 0), 750, &Limits::REGIONAL, |x, z| {
            ridge.get(&(x, z)).copied()
        });
        let read = |x, z| ridge.get(&(x, z)).copied();
        let commands = batch(&edits, (0, 0), 2, BATCH, read);
        assert!(
            matches!(commands.as_slice(), [Command::Cells(cells)] if cells.len() == 13),
            "mixed surfaces are one batch that leaves the stone cell's kind alone"
        );
        assert!(batch(&[], (0, 0), 2, BATCH, |_, _| None).is_empty());

        // A profile without the batch command still goes cell by cell.
        let legacy = Limits::REGIONAL.per_cell();
        assert_eq!(legacy.max_batch, 0);
        let commands = batch(&edits, (0, 0), 2, legacy.max_batch, read);
        assert_eq!(commands.len(), 13);
        assert!(
            commands
                .iter()
                .all(|command| matches!(command, Command::Disc { radius: 0, .. }))
        );
        let mut by_cell = ridge.clone();
        apply(&mut by_cell, &commands);
        let mut batched = ridge.clone();
        apply(&mut batched, &batch(&edits, (0, 0), 2, BATCH, read));
        assert_eq!(by_cell, batched, "both shapes record the same terrain");
    }

    #[test]
    fn every_tool_records_one_command_per_dab_that_reproduces_its_plan() {
        for tool in Tool::ALL {
            for falloff in [Falloff::Soft, Falloff::Hard] {
                let mut map = rough();
                let brush = Brush {
                    tool,
                    radius: 3,
                    falloff,
                    surface: Stone,
                    step_mm: 250,
                };
                let read = |x, z| map.get(&(x, z)).copied();
                let edits = plan_dab(&brush, (0, 0), 1400, &Limits::REGIONAL, read);
                assert!(!edits.is_empty(), "{tool:?} changes rough ground");
                let commands = batch(&edits, (0, 0), 3, BATCH, read);
                let [Command::Cells(cells)] = commands.as_slice() else {
                    panic!("{tool:?} {falloff:?}: one batch, not {commands:?}");
                };
                // Exactly the changed cells, each with its planned value.
                let planned: Vec<_> = edits
                    .iter()
                    .map(|edit| ((edit.x, edit.z), edit.after))
                    .collect();
                assert_eq!(cells, &planned, "{tool:?} {falloff:?}");
                assert!(cells.len() <= 29, "bounded by the brush area");
                let mut expected = map.clone();
                expected.extend(planned);
                apply(&mut map, &commands);
                assert_eq!(map, expected, "{tool:?} {falloff:?}");
            }
        }
        // On level ground a hard brush is the single disc it always was;
        // smoothing level ground records nothing.
        let level = flat(1000, Grass, 8);
        let read = |x, z| level.get(&(x, z)).copied();
        for tool in Tool::ALL {
            let brush = Brush {
                tool,
                radius: 3,
                falloff: Falloff::Hard,
                surface: Stone,
                step_mm: 250,
            };
            let edits = plan_dab(&brush, (0, 0), 1400, &Limits::REGIONAL, read);
            let commands = batch(&edits, (0, 0), 3, BATCH, read);
            match tool {
                Tool::Smooth => assert!(commands.is_empty()),
                _ => assert!(
                    matches!(commands.as_slice(), [Command::Disc { radius: 3, .. }]),
                    "{tool:?}: {commands:?}"
                ),
            }
        }
    }

    #[test]
    fn undo_and_redo_are_one_inverse_batch_each_from_captured_cells() {
        let original = rough();
        let mut map = original.clone();
        let mut history = History::new(8);
        let mut stroke = Stroke::default();
        // Overlapping dabs of one drag, with two tools' worth of change.
        for (tool, center) in [
            (Tool::Raise, (0, 0)),
            (Tool::Raise, (1, 0)),
            (Tool::Smooth, (1, 1)),
        ] {
            let edits = plan_dab(
                &brush(tool, 2, Falloff::Soft),
                center,
                0,
                &Limits::REGIONAL,
                |x, z| map.get(&(x, z)).copied(),
            );
            let commands = batch(&edits, center, 2, BATCH, |x, z| map.get(&(x, z)).copied());
            assert_eq!(commands.len(), 1);
            assert_eq!(stroke.len_with(&edits), {
                let mut probe = stroke.clone();
                probe.absorb(&edits);
                probe.len()
            });
            apply(&mut map, &commands);
            stroke.absorb(&edits);
        }
        let touched = stroke.len();
        assert!(touched > 13 && touched < 39, "overlaps are captured once");
        let sculpted = map.clone();
        assert_ne!(sculpted, original);
        history.push(stroke);
        assert!(history.can_undo() && !history.can_redo());

        let undo = history
            .undo(BATCH, |x, z| map.get(&(x, z)).copied())
            .unwrap();
        let [Command::Cells(cells)] = undo.commands.as_slice() else {
            panic!("undo is one batch, not {:?}", undo.commands);
        };
        assert_eq!(cells.len(), touched);
        assert!(
            cells.iter().all(|(at, value)| original[at] == *value),
            "each cell returns to its first captured value"
        );
        assert_eq!(undo.expect.len(), touched);
        apply(&mut map, &undo.commands);
        assert_eq!(map, original, "undo restores the first captured values");
        assert!(history.can_redo() && !history.can_undo());

        let redo = history
            .redo(BATCH, |x, z| map.get(&(x, z)).copied())
            .unwrap();
        assert!(
            matches!(redo.commands.as_slice(), [Command::Cells(cells)] if cells.len() == touched)
        );
        assert_eq!(redo.stroke, undo.stroke);
        apply(&mut map, &redo.commands);
        assert_eq!(map, sculpted, "redo restores the latest values");

        // Without the batch command the same step is one command per cell.
        let legacy = history.undo(0, |x, z| map.get(&(x, z)).copied()).unwrap();
        assert_eq!(legacy.commands.len(), touched);
        let mut by_cell = map.clone();
        apply(&mut by_cell, &legacy.commands);
        assert_eq!(by_cell, original);
        assert!(history.undo(BATCH, |_, _| None).is_none());
    }

    #[test]
    fn a_refused_step_is_taken_back_unless_a_new_stroke_followed() {
        let map = flat(0, Grass, 4);
        let stroke_at = |x: i32| {
            let mut stroke = Stroke::default();
            stroke.absorb(&[Edit {
                x,
                z: 0,
                before: map[&(0, 0)],
                after: Cell {
                    height_mm: 250,
                    surface: Grass,
                },
            }]);
            stroke
        };
        let mut history = History::new(4);
        history.push(stroke_at(0));
        history.push(stroke_at(1));
        let undo = history.undo(BATCH, |_, _| None).unwrap();
        assert!(history.can_redo());
        // The simulation refused it: the stroke is undoable again.
        assert!(history.take_back(undo.stroke, Step::Undo));
        assert!(!history.can_redo());
        let again = history.undo(BATCH, |_, _| None).unwrap();
        assert_eq!(again.stroke, undo.stroke);
        let redo = history.redo(BATCH, |_, _| None).unwrap();
        assert!(history.take_back(redo.stroke, Step::Redo));
        assert!(history.can_redo(), "a refused redo can be tried again");
        assert!(!history.take_back(redo.stroke, Step::Redo), "only once");
        // A stroke drawn while the step was in flight started a new future.
        let stale = history.undo(BATCH, |_, _| None).unwrap();
        history.push(stroke_at(2));
        assert!(!history.take_back(stale.stroke, Step::Undo));
        assert!(!history.take_back(stale.stroke, Step::Redo));
        assert!(history.can_undo() && !history.can_redo());
    }

    #[test]
    fn a_refused_dab_leaves_the_stroke_so_its_redo_is_what_was_applied() {
        let ground = Cell {
            height_mm: 1000,
            surface: Grass,
        };
        let raised = |height_mm| Cell {
            height_mm,
            surface: Grass,
        };
        let edit = |x, before: Cell, after: Cell| Edit {
            x,
            z: 0,
            before,
            after,
        };
        // Three dabs: the first is applied, the second (on two cells) is
        // refused, the third writes one of the second's cells again.
        let first = [edit(0, ground, raised(1250))];
        let second = [
            edit(0, raised(1250), raised(1500)),
            edit(1, ground, raised(1250)),
            edit(2, ground, raised(1250)),
        ];
        let third = [edit(2, raised(1250), raised(1400))];
        let mut stroke = Stroke::default();
        stroke.absorb(&first);
        stroke.absorb(&second);
        stroke.absorb(&third);
        assert_eq!(stroke.len(), 3);
        assert!(stroke.retract(&second));
        // Cell 0 is back at the first dab's value, cell 1 never changed and
        // has left, cell 2 keeps what the later dab wrote.
        assert_eq!(
            stroke.expectations(false),
            [((0, 0), raised(1250)), ((2, 0), raised(1400))]
        );
        assert!(!stroke.retract(&second), "taken back once only");
        // In the history: the newest stroke that holds the dab gives it up,
        // and a stroke left with nothing is no history at all.
        let mut history = History::new(4);
        let mut whole = Stroke::default();
        whole.absorb(&first);
        history.push(whole);
        let mut refused = Stroke::default();
        refused.absorb(&[edit(5, ground, raised(1250))]);
        history.push(refused);
        assert_eq!(history.depth().0, 2);
        history.retract(&[edit(5, ground, raised(1250))]);
        assert_eq!(history.depth().0, 1, "the refused stroke is gone");
        let map: Map = [((0, 0), raised(1250))].into_iter().collect();
        let undo = history
            .undo(BATCH, |x, z| map.get(&(x, z)).copied())
            .unwrap();
        assert_eq!(undo.expect, [((0, 0), ground)]);
    }

    #[test]
    fn history_is_bounded_and_a_new_stroke_discards_redo() {
        let map = flat(0, Grass, 4);
        let stroke_at = |x: i32| {
            let mut stroke = Stroke::default();
            stroke.absorb(&[Edit {
                x,
                z: 0,
                before: map[&(0, 0)],
                after: Cell {
                    height_mm: 250,
                    surface: Grass,
                },
            }]);
            stroke
        };
        let mut history = History::new(2);
        history.push(Stroke::default());
        assert!(!history.can_undo(), "an empty stroke is not history");
        for x in 0..3 {
            history.push(stroke_at(x));
        }
        let first = history
            .undo(BATCH, |x, z| map.get(&(x, z)).copied())
            .unwrap();
        assert_eq!(first.commands.len(), 0, "the sample already holds `before`");
        assert_eq!(first.expect[0].0, (2, 0), "newest stroke first");
        assert!(history.undo(BATCH, |_, _| None).is_some());
        assert!(
            history.undo(BATCH, |_, _| None).is_none(),
            "the oldest stroke fell out of the bounded history"
        );
        assert!(history.can_redo());
        history.push(stroke_at(3));
        assert!(!history.can_redo(), "a new stroke starts a new future");
    }
}
