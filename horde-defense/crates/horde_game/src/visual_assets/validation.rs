//! Whether a loaded sheet can back the rectangles measured on it.
use super::*;

/// Why `image` cannot back the rectangles measured on it, if it cannot: it must
/// have exactly the declared pixel size and contain every rectangle.
pub(super) fn sheet_problem(
    image: &Image,
    name: &str,
    declared: [f32; 2],
    rects: &[[f32; 4]],
) -> Option<String> {
    let size = [image.width() as f32, image.height() as f32];
    if size != declared {
        return Some(format!(
            "{name} is {}x{} but its art metadata declares {declared:?}",
            image.width(),
            image.height()
        ));
    }
    rects
        .iter()
        .find(|r| r[2] > size[0] || r[3] > size[1] || r[0] < 0.0 || r[1] < 0.0)
        .map(|r| format!("{r:?} leaves {name}"))
}
/// Why the loaded sheets cannot back the enemy mapping, if they cannot: each
/// sheet a sprite is measured on must fit as `sheet_problem` requires. Checks
/// only the sheets that have loaded.
pub(super) fn enemy_art_problem(walkers: Option<&Image>, actors: Option<&Image>) -> Option<String> {
    for sprite in enemy_visuals::active().sprites() {
        let (image, name, rects): (_, _, Vec<[f32; 4]>) = match &sprite.art {
            SpriteArt::Walk(walk) => (
                walkers,
                "walkers-v1.png",
                walk.frames.iter().map(|frame| frame.bounds).collect(),
            ),
            SpriteArt::Standing(art) => (actors, "actors-v1.png", vec![art.bounds]),
        };
        let Some(image) = image else {
            continue;
        };
        if let Some(problem) = sheet_problem(image, name, sprite.source_size, &rects) {
            return Some(format!("{problem} ({:?})", sprite.sprite));
        }
    }
    None
}
/// Player, companion, portrait and corpse cells share the actor sheet; a
/// mismatch draws the procedural survivor figures instead.
pub(super) fn survivor_art_problem(actors: &Image) -> Option<String> {
    let rects: Vec<[f32; 4]> = SURVIVOR_RECTS
        .iter()
        .chain(&PORTRAIT_RECTS)
        .chain([&CORPSE_RECT])
        .copied()
        .collect();
    sheet_problem(actors, "actors-v1.png", ACTOR_SIZE, &rects)
}
/// A mismatched prop sheet draws procedural props instead.
pub(super) fn prop_art_problem(props: &Image) -> Option<String> {
    sheet_problem(props, "props-v1.png", PROP_SIZE, &PROP_RECTS)
}
