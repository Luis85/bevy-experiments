use horde_core::{content::StructureKind, sim::Game};

/// The construction cell under the aim and whether a structure may go there.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementPreview {
    pub lane: u8,
    pub column: u8,
    pub center: [f32; 2],
    pub half_width: f32,
    pub valid: Result<(), String>,
    /// The structure already standing in the cell, which salvage removes.
    pub occupant: Option<u64>,
}
/// `None` outside a battle or when the aim is off the construction grid.
pub fn placement_preview(
    game: &Game,
    kind: StructureKind,
    aim: [f32; 2],
) -> Option<PlacementPreview> {
    let a = game.attempt()?;
    let [lane, column] = a.build_cell_at(aim)?;
    Some(PlacementPreview {
        lane,
        column,
        center: a.build_cell_center(lane, column).expect("validated cell"),
        half_width: a.construction_cell_width() * 0.5,
        valid: game.preview_placement(kind, lane, column),
        occupant: a
            .structures
            .iter()
            .find(|s| s.lane == lane && s.column == column)
            .map(|s| s.id),
    })
}
