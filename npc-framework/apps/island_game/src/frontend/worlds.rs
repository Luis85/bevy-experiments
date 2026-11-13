//! The New-game world list: the world this window opened with, then every
//! world Shape exported beside the island rules, newest first. Host files
//! only; the chosen file goes to `session::new_game_on`, which checks it the
//! way every new island is checked.
use bevy::prelude::*;
use island_content::worlds::WorldPreset;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One world the New-game screen offers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorldEntry {
    /// `None`: the world this window opened with.
    pub(crate) path: Option<PathBuf>,
    pub(crate) name: String,
    /// "Shipped world", or the file's date: "12 Nov 2026".
    pub(crate) when: String,
    /// The file's name, for an exported world.
    pub(crate) file: Option<String>,
    /// "3 settlements · patches: 4 logs, 2 grain"; for a file that cannot be
    /// used, why not.
    pub(crate) summary: String,
    /// Why it cannot be used. Such a world is listed, disabled.
    pub(crate) problem: Option<String>,
}

/// The list as last read, and which entry the player chose.
#[derive(Resource, Default)]
pub(crate) struct WorldChoice {
    pub(crate) entries: Vec<WorldEntry>,
    pub(crate) chosen: usize,
}

impl WorldChoice {
    /// The chosen world's file; `None` for the default world.
    pub(crate) fn chosen_path(&self) -> Option<PathBuf> {
        self.entries
            .get(self.chosen)
            .filter(|entry| entry.problem.is_none())
            .and_then(|entry| entry.path.clone())
    }
    /// Read the list again, keeping the choice where it still exists, or
    /// choosing `prefer` when given.
    pub(crate) fn reload(
        &mut self,
        default: &WorldPreset,
        folder: Option<&Path>,
        prefer: Option<&Path>,
    ) {
        let kept = prefer
            .map(Path::to_path_buf)
            .or_else(|| self.entries.get(self.chosen).and_then(|e| e.path.clone()));
        self.entries = list(default, folder);
        self.chosen = kept
            .and_then(|path| {
                self.entries.iter().position(|entry| {
                    entry.path.as_deref() == Some(path.as_path()) && entry.problem.is_none()
                })
            })
            .unwrap_or(0);
    }
}

/// Whether a file name is one Shape's export writes: `<slug>-shaped-N.json`.
pub(crate) fn is_exported_name(name: &str) -> bool {
    name.strip_suffix(".json")
        .and_then(|stem| stem.rsplit_once("-shaped-"))
        .is_some_and(|(slug, n)| {
            !slug.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
        })
}

/// The default world first, then exported worlds newest first. A file that
/// cannot be read or used is listed with the reason, never left out.
pub(crate) fn list(default: &WorldPreset, folder: Option<&Path>) -> Vec<WorldEntry> {
    let mut out = vec![WorldEntry {
        path: None,
        name: default.name.clone(),
        when: if default.name.ends_with("(shaped)") {
            "This window's world".into()
        } else {
            "Shipped world".into()
        },
        summary: summary(default),
        file: None,
        problem: None,
    }];
    let Some(folder) = folder else {
        return out;
    };
    let Ok(read) = std::fs::read_dir(folder) else {
        return out;
    };
    let mut files: Vec<(Option<SystemTime>, PathBuf)> = read
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_exported_name)
        })
        .map(|path| {
            (
                std::fs::metadata(&path).and_then(|m| m.modified()).ok(),
                path,
            )
        })
        .collect();
    // Newest first; the same moment falls back to the name, highest first.
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    for (modified, path) in files {
        let file = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let when = modified.map_or_else(|| file.clone(), date);
        out.push(match crate::island::world_file_preset(&path) {
            Ok(preset) => WorldEntry {
                path: Some(path),
                name: preset.name.clone(),
                when,
                summary: summary(&preset),
                file: Some(file),
                problem: None,
            },
            Err(reason) => {
                // The parser's own words go to the log, not the card.
                bevy::log::warn!("World file {} cannot be used: {reason}", path.display());
                WorldEntry {
                    path: Some(path),
                    name: file.clone(),
                    when,
                    summary: format!("Cannot be used: {}", plain_reason(&reason)),
                    file: None,
                    problem: Some(reason),
                }
            }
        });
    }
    out
}

/// Why a file cannot be used, in plain words and without the parser's
/// detail: "it is not a world file.", "it cannot be read.", or "its rules
/// do not pass the world checks."
pub(crate) fn plain_reason(reason: &str) -> String {
    for plain in ["it is not a world file", "it cannot be read"] {
        if reason.starts_with(plain) {
            return format!("{plain}.");
        }
    }
    "its rules do not pass the world checks.".to_owned()
}

/// "3 settlements · patches: 4 logs, 2 grain".
pub(crate) fn summary(preset: &WorldPreset) -> String {
    let Some(living) = &preset.living_world else {
        return "The island colony".into();
    };
    let settlements = living.settlements.len();
    let mut goods: BTreeMap<&str, usize> = BTreeMap::new();
    if living.resource_patches.is_empty() {
        // Every settlement gets the economy's list, by biome.
        for patch in &living.economy.patches {
            let count = living
                .settlements
                .iter()
                .filter(|s| patch.biomes.is_empty() || patch.biomes.contains(&s.biome))
                .count();
            *goods.entry(patch.good.as_str()).or_default() += count;
        }
    } else {
        for patch in &living.resource_patches {
            *goods.entry(patch.good.as_str()).or_default() += 1;
        }
    }
    let mut goods: Vec<(&str, usize)> = goods.into_iter().filter(|(_, n)| *n > 0).collect();
    goods.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let patches = goods
        .iter()
        .map(|(good, n)| {
            island_view::game_ui::words::count(u64::try_from(*n).unwrap_or(u64::MAX), good)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let places = if settlements == 1 {
        "1 settlement".to_owned()
    } else {
        format!("{settlements} settlements")
    };
    if patches.is_empty() {
        places
    } else {
        format!("{places} \u{b7} patches: {patches}")
    }
}

/// A file date as "12 Nov 2026" (UTC; host data only).
pub(crate) fn date(time: SystemTime) -> String {
    let days = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    let (y, m, d) = civil(days);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{d} {} {y}", MONTHS[(m - 1) as usize])
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn living() -> WorldPreset {
        island_content::worlds::builtin_preset("living_world").unwrap()
    }

    #[test]
    fn exported_names_are_recognised_and_others_are_not() {
        assert!(is_exported_name("the-living-island-shaped-1.json"));
        assert!(is_exported_name("a-shaped-12.json"));
        assert!(!is_exported_name("living_world.json"));
        assert!(!is_exported_name("a-shaped-.json"));
        assert!(!is_exported_name("-shaped-1.json"));
        assert!(!is_exported_name("a-shaped-1.json.partial"));
    }

    #[test]
    fn dates_read_as_a_day_month_and_year() {
        let day = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_794_441_600);
        assert_eq!(date(day), "12 Nov 2026");
        assert_eq!(date(SystemTime::UNIX_EPOCH), "1 Jan 1970");
    }

    #[test]
    fn the_list_puts_the_default_first_then_exports_newest_first_and_keeps_bad_files() {
        let dir = std::env::temp_dir().join(format!("npc-world-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut shaped = living();
        shaped.name = "Older (shaped)".into();
        let write = |name: &str, preset: &WorldPreset| {
            std::fs::write(dir.join(name), serde_json::to_string(preset).unwrap()).unwrap();
        };
        write("older-shaped-1.json", &shaped);
        // A later modification time for the second file.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        shaped.name = "Newer (shaped)".into();
        write("newer-shaped-1.json", &shaped);
        std::fs::write(dir.join("broken-shaped-1.json"), "{ not json").unwrap();
        // Not an export: never listed.
        write("living_world.json", &living());
        let entries = list(&living(), Some(&dir));
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names[0], living().name);
        assert_eq!(entries[0].path, None);
        assert_eq!(entries[0].when, "Shipped world");
        assert_eq!(entries.len(), 4, "{names:?}");
        let newer = names.iter().position(|n| *n == "Newer (shaped)").unwrap();
        let older = names.iter().position(|n| *n == "Older (shaped)").unwrap();
        assert!(newer < older, "newest first: {names:?}");
        let broken = entries
            .iter()
            .find(|e| e.name == "broken-shaped-1.json")
            .expect("an unreadable file is listed");
        assert!(
            broken
                .problem
                .as_deref()
                .unwrap()
                .contains("not a world file")
        );
        assert_eq!(broken.summary, "Cannot be used: it is not a world file.");
        assert!(
            entries[newer].summary.contains("settlements"),
            "{}",
            entries[newer].summary
        );
        // A bad file cannot be chosen; the choice falls back to the default.
        let mut choice = WorldChoice::default();
        choice.reload(
            &living(),
            Some(&dir),
            Some(&dir.join("broken-shaped-1.json")),
        );
        assert_eq!(choice.chosen, 0);
        assert_eq!(choice.chosen_path(), None);
        choice.reload(
            &living(),
            Some(&dir),
            Some(&dir.join("newer-shaped-1.json")),
        );
        assert_eq!(choice.chosen_path(), Some(dir.join("newer-shaped-1.json")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_summary_counts_settlements_and_patches_by_good() {
        let mut preset = living();
        let line = summary(&preset);
        assert!(line.contains("settlements \u{b7} patches:"), "{line}");
        let living = preset.living_world.as_mut().unwrap();
        living.resource_patches = vec![
            island_content::village::frontier::world_spec::PlacedPatch {
                good: "logs".into(),
                x: 0,
                z: 0,
                capacity: 10,
                settlement: living.settlements[0].key.clone(),
            };
            2
        ];
        assert!(
            summary(&preset).ends_with("patches: 2 logs"),
            "{}",
            summary(&preset)
        );
    }
}
