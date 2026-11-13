//! Random names for agents and worlds, picked from fixed word lists by seed.

use npc_framework::kernel::rng::splitmix64;

/// What a name is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameKind {
    /// An explorer agent: "Wren Ashdown".
    Agent,
    /// A world: "Hollowmere".
    World,
}

const AGENT_FIRST: &[&str] = &[
    "Wren", "Brin", "Tamsin", "Corvin", "Maren", "Ossian", "Lark", "Fenn", "Isolde", "Garrick",
    "Perrin", "Sorrel", "Dunstan", "Elowen", "Hale", "Idris", "Juniper", "Kestrel", "Linnea",
    "Merrick", "Nell", "Orrin", "Pippa", "Quill", "Rowan", "Sable", "Tobin", "Una", "Vesper",
    "Wystan", "Yarrow", "Zephyr", "Aldous", "Bryony", "Cassia", "Dorian", "Embla", "Finch", "Gwen",
    "Heron", "Ivo", "Jessamy",
];

const AGENT_LAST: &[&str] = &[
    "Ashdown",
    "Thornfield",
    "Marlow",
    "Greywater",
    "Holloway",
    "Winterbourne",
    "Fairweather",
    "Stonecrop",
    "Brightwell",
    "Oakhart",
    "Larkspur",
    "Ravenscroft",
    "Dunmore",
    "Elmsworth",
    "Foxglove",
    "Hartley",
    "Ironwood",
    "Kettleby",
    "Longmead",
    "Mossgrove",
    "Nettlebed",
    "Overton",
    "Pennyfeather",
    "Quarrington",
    "Redfern",
    "Saltmarsh",
    "Tallow",
    "Underhill",
    "Vance",
    "Wickham",
    "Yewdale",
    "Applegarth",
    "Blackthorn",
    "Copperfield",
    "Dewhurst",
    "Everard",
    "Fernsby",
    "Goldfinch",
    "Hazelmere",
    "Inglenook",
    "Juniper",
    "Kingfisher",
];

const WORLD_FIRST: &[&str] = &[
    "Hollow", "Misty", "Amber", "Ember", "Silver", "Willow", "Raven", "Stone", "Frost", "Briar",
    "Cinder", "Dusk", "Elder", "Fallow", "Gilded", "Heather", "Iron", "Juniper", "Kelp", "Lantern",
    "Moss", "Nettle", "Oak", "Pebble", "Quartz", "Rook", "Sedge", "Thistle", "Umber", "Violet",
    "Wolf", "Yew", "Ash", "Bracken", "Copper", "Drift", "Eve", "Flint", "Gorse", "Harrow",
];

const WORLD_LAST: &[&str] = &[
    "mere", "vale", "wick", "ford", "holm", "reach", "fell", "moor", "haven", "stead", "wood",
    "brook", "crest", "dale", "field", "gate", "hollow", "isle", "marsh", "ness", "pool", "ridge",
    "shaw", "thorpe", "watch", "water", "combe", "downs", "fen", "glen", "heath", "keep", "lea",
    "mount", "pass", "run", "scar", "tarn", "weald", "wold",
];

fn pick(list: &[&'static str], h: u64) -> &'static str {
    list[(h % list.len() as u64) as usize]
}

/// A name for `kind`, a pure function of `seed`.
pub fn random_name(kind: NameKind, seed: u64) -> String {
    let a = splitmix64(seed);
    let b = splitmix64(a ^ 0xA5A5_5A5A_1234_5678);
    match kind {
        NameKind::Agent => format!("{} {}", pick(AGENT_FIRST, a), pick(AGENT_LAST, b)),
        NameKind::World => format!("{}{}", pick(WORLD_FIRST, a), pick(WORLD_LAST, b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_name_is_stable_per_seed_and_varies() {
        for kind in [NameKind::Agent, NameKind::World] {
            assert_eq!(random_name(kind, 7), random_name(kind, 7));
            let distinct: std::collections::BTreeSet<String> =
                (0..50).map(|s| random_name(kind, s)).collect();
            assert!(distinct.len() > 20, "{kind:?}: {}", distinct.len());
        }
        assert!(random_name(NameKind::Agent, 1).contains(' '));
        assert!(!random_name(NameKind::World, 1).contains(' '));
    }

    #[test]
    fn word_lists_are_long_enough() {
        for l in [AGENT_FIRST, AGENT_LAST, WORLD_FIRST, WORLD_LAST] {
            assert!(l.len() >= 40, "{}", l.len());
        }
    }
}
