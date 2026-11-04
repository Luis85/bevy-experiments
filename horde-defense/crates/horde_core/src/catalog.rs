//! Shared plumbing for the authored catalogues: one override table, one error type,
//! and the fallible load/install path used by every `try_load()`.
use serde::de::DeserializeOwned;
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// Every authored catalogue and the environment variable that overrides its bundled
/// default. The single source for the loaders and the startup diagnostic console.
pub const CATALOG_OVERRIDES: &[(&str, &str)] = &[
    ("content", "LASTLIGHT_CONTENT"),
    ("world", "LASTLIGHT_WORLD"),
    ("skills", "LASTLIGHT_SKILLS"),
    ("enemies", "LASTLIGHT_ENEMIES"),
    ("ordnance", "LASTLIGHT_ORDNANCE"),
    ("progression", "LASTLIGHT_PROGRESSION"),
    ("quality", "LASTLIGHT_QUALITY"),
    ("balance", "LASTLIGHT_BALANCE"),
];

pub(crate) const MIB: u64 = 1024 * 1024;

#[derive(Debug)]
pub enum ContentError {
    /// The override file could not be inspected or read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The text is not valid JSON for this catalogue's schema.
    Parse {
        catalog: &'static str,
        message: String,
    },
    /// The text parsed but failed the catalogue's validation rules.
    Invalid {
        catalog: &'static str,
        message: String,
    },
    /// The authored text is larger than the supported authoring limit.
    TooLarge {
        catalog: &'static str,
        limit: u64,
        actual: u64,
    },
}

impl ContentError {
    /// Plain message without the catalogue prefix, for the legacy `String` APIs.
    pub fn into_message(self) -> String {
        match self {
            Self::Parse { message, .. } | Self::Invalid { message, .. } => message,
            other => other.to_string(),
        }
    }
    fn with_source(self, source: &str) -> Self {
        match self {
            Self::Parse { catalog, message } => Self::Parse {
                catalog,
                message: format!("{source}: {message}"),
            },
            Self::Invalid { catalog, message } => Self::Invalid {
                catalog,
                message: format!("{source}: {message}"),
            },
            other => other,
        }
    }
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    f,
                    "Could not read catalogue file {}: {source}",
                    path.display()
                )
            }
            Self::Parse { catalog, message } => {
                write!(f, "Cannot parse {catalog} catalogue: {message}")
            }
            Self::Invalid { catalog, message } => {
                write!(f, "Invalid {catalog} catalogue: {message}")
            }
            Self::TooLarge {
                catalog,
                limit,
                actual,
            } => write!(
                f,
                "The {catalog} catalogue is {actual} bytes, over the supported {} MiB authoring limit",
                limit / MIB
            ),
        }
    }
}

impl std::error::Error for ContentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Where one catalogue came from, for the startup diagnostic console.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogSource {
    pub name: &'static str,
    pub env_var: &'static str,
    /// The override path, or `None` for the bundled default.
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogReport {
    pub catalogs: Vec<CatalogSource>,
}

fn override_var(name: &str) -> &'static str {
    CATALOG_OVERRIDES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, var)| *var)
        .unwrap_or_else(|| panic!("catalogue {name} missing from CATALOG_OVERRIDES"))
}

/// Empty values count as unset, matching the frontend's environment handling.
fn override_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(override_var(name))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Pure parse + validate for JSON catalogues, so alternative catalogues can be built in-process.
pub(crate) fn parse_validated<T: DeserializeOwned>(
    catalog: &'static str,
    text: &str,
    limit: u64,
    validate: impl FnOnce(&T) -> Result<(), String>,
) -> Result<T, ContentError> {
    if text.len() as u64 > limit {
        return Err(ContentError::TooLarge {
            catalog,
            limit,
            actual: text.len() as u64,
        });
    }
    let value: T = serde_json::from_str(text).map_err(|e| ContentError::Parse {
        catalog,
        message: e.to_string(),
    })?;
    validate(&value).map_err(|message| ContentError::Invalid { catalog, message })?;
    Ok(value)
}

fn read_override(path: &Path, catalog: &'static str, limit: u64) -> Result<String, ContentError> {
    let io = |source| ContentError::Io {
        path: path.to_path_buf(),
        source,
    };
    let len = std::fs::metadata(path).map_err(io)?.len();
    if len > limit {
        return Err(ContentError::TooLarge {
            catalog,
            limit,
            actual: len,
        });
    }
    std::fs::read_to_string(path).map_err(io)
}

/// Load a catalogue from its environment override, else from the bundled text.
pub(crate) fn load<T>(
    catalog: &'static str,
    limit: u64,
    bundled: &str,
    parse: impl FnOnce(&str) -> Result<T, ContentError>,
) -> Result<T, ContentError> {
    match override_path(catalog) {
        Some(path) => {
            let text = read_override(&path, catalog, limit)?;
            let source = format!("{} {}", override_var(catalog), path.display());
            parse(&text).map_err(|e| e.with_source(&source))
        }
        None => parse(bundled),
    }
}

/// Lazy accessor body: honour a valid override, else fall back to the bundled default.
/// An invalid override is reported once; `init_catalogs()` is the strict startup path.
pub(crate) fn lazy<T>(
    cell: &'static OnceLock<T>,
    catalog: &'static str,
    try_load: impl FnOnce() -> Result<T, ContentError>,
    bundled: impl FnOnce() -> Result<T, ContentError>,
) -> &'static T {
    cell.get_or_init(|| {
        try_load().unwrap_or_else(|error| {
            tracing::error!(%error, catalog, "invalid catalogue override; using bundled default");
            bundled().expect("bundled catalogue is valid")
        })
    })
}

/// Strict install: load (override or bundled), validate, and publish into the cell.
pub(crate) fn install<T>(
    cell: &'static OnceLock<T>,
    try_load: impl FnOnce() -> Result<T, ContentError>,
) -> Result<&'static T, ContentError> {
    if let Some(value) = cell.get() {
        return Ok(value);
    }
    let value = try_load()?;
    Ok(cell.get_or_init(|| value))
}

/// Stable FNV-1a hash of the loaded catalogues' authored IDs that saves can
/// reference: items, levels and structures (in authored order, since loot and
/// stage selection are positional), countries, enemy archetypes and skill nodes.
/// Stored in save envelopes so a load against different content is diagnosable.
pub fn content_fingerprint() -> u64 {
    let content = crate::content::content();
    let sections: [(&str, Vec<&str>); 6] = [
        (
            "items",
            content.items.iter().map(|i| i.id.as_str()).collect(),
        ),
        (
            "levels",
            content.levels.iter().map(|l| l.id.as_str()).collect(),
        ),
        (
            "structures",
            content.structures.iter().map(|s| s.id.as_str()).collect(),
        ),
        (
            "countries",
            crate::world::world()
                .countries
                .iter()
                .map(|c| c.id.as_str())
                .collect(),
        ),
        (
            "enemies",
            crate::enemy_catalog::enemy_catalog()
                .archetype_ids()
                .collect(),
        ),
        (
            "skills",
            crate::skill_catalog::skills()
                .nodes
                .iter()
                .map(|n| n.id.as_str())
                .collect(),
        ),
    ];
    let mut hash = 0xcbf29ce484222325_u64;
    for (section, ids) in sections {
        // Separators keep ("ab", "c") distinct from ("a", "bc").
        for byte in std::iter::once(section)
            .chain(ids)
            .flat_map(|id| id.bytes().chain([0xff]))
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        hash = (hash ^ 0xfe).wrapping_mul(0x100000001b3);
    }
    hash
}

/// Load and validate every catalogue, installing them for the process-wide accessors.
/// Call once at startup so a bad override is a clean error instead of a later panic.
pub fn init_catalogs() -> Result<CatalogReport, ContentError> {
    crate::content::install()?;
    crate::world::install()?;
    crate::skill_catalog::install()?;
    crate::enemy_catalog::install()?;
    crate::ordnance::install()?;
    crate::progression::install()?;
    crate::quality::install()?;
    crate::balance::install()?;
    Ok(CatalogReport {
        catalogs: CATALOG_OVERRIDES
            .iter()
            .map(|&(name, env_var)| CatalogSource {
                name,
                env_var,
                path: override_path(name),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize)]
    struct Tiny {
        n: u32,
    }

    #[test]
    fn content_fingerprint_is_deterministic_and_covers_item_ids() {
        assert_eq!(content_fingerprint(), content_fingerprint());
        assert_ne!(content_fingerprint(), 0xcbf29ce484222325);
    }

    #[test]
    fn invalid_json_is_a_parse_error() {
        let err = parse_validated::<Tiny>("tiny", "{ not json", MIB, |_| Ok(())).unwrap_err();
        assert!(
            matches!(
                err,
                ContentError::Parse {
                    catalog: "tiny",
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn failed_validation_is_an_invalid_error() {
        let err = parse_validated::<Tiny>("tiny", r#"{"n": 0}"#, MIB, |t| {
            if t.n == 0 {
                Err("n must be positive".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(matches!(err, ContentError::Invalid { .. }), "{err}");
        assert!(err.to_string().contains("n must be positive"));
    }

    #[test]
    fn oversized_text_is_rejected_before_parsing() {
        let err = parse_validated::<Tiny>("tiny", &" ".repeat(17), 16, |_| Ok(())).unwrap_err();
        assert!(matches!(
            err,
            ContentError::TooLarge {
                limit: 16,
                actual: 17,
                ..
            }
        ));
    }

    #[test]
    fn missing_override_file_is_an_io_error_naming_the_path() {
        let path = Path::new("definitely/not/here.json");
        let err = read_override(path, "tiny", MIB).unwrap_err();
        assert!(matches!(err, ContentError::Io { .. }));
        assert!(err.to_string().contains("here.json"));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn every_catalogue_has_exactly_one_distinct_override() {
        let mut vars: Vec<_> = CATALOG_OVERRIDES.iter().map(|(_, v)| *v).collect();
        vars.sort_unstable();
        vars.dedup();
        assert_eq!(vars.len(), CATALOG_OVERRIDES.len());
        assert!(vars.iter().all(|v| v.starts_with("LASTLIGHT_")));
    }

    #[test]
    fn each_catalogue_rejects_bad_override_text_with_a_typed_error() {
        let bad = "{ not json";
        assert!(matches!(
            crate::content::Content::from_json_str(bad),
            Err(ContentError::Parse { .. })
        ));
        assert!(matches!(
            crate::world::WorldContent::from_json_str(bad),
            Err(ContentError::Parse { .. })
        ));
        assert!(matches!(
            crate::skill_catalog::SkillCatalog::from_json_str(bad),
            Err(ContentError::Parse { .. })
        ));
        assert!(matches!(
            crate::enemy_catalog::EnemyCatalog::from_json_str(bad),
            Err(ContentError::Parse { .. })
        ));
        assert!(matches!(
            crate::ordnance::OrdnanceCatalog::from_json_str(bad),
            Err(ContentError::Parse { .. })
        ));
        assert!(matches!(
            crate::progression::ProgressionCatalog::from_json_str(bad),
            Err(ContentError::Parse { .. })
        ));
        assert!(matches!(
            crate::quality::QualityCatalog::from_json_str(bad),
            Err(ContentError::Parse { .. })
        ));
        assert!(matches!(
            crate::balance::BalanceCatalog::from_json_str(bad),
            Err(ContentError::Parse { .. })
        ));
    }

    #[test]
    fn schema_valid_but_rule_breaking_content_is_invalid_not_parse() {
        let mut data = crate::content::content().clone();
        data.items[0].personal.magazine = 0;
        let text = serde_json::to_string(&data).unwrap();
        assert!(matches!(
            crate::content::Content::from_json_str(&text),
            Err(ContentError::Invalid {
                catalog: "content",
                ..
            })
        ));
    }

    #[test]
    fn bundled_catalogues_pass_strict_init() {
        let report = init_catalogs().expect("bundled catalogues are valid");
        assert_eq!(report.catalogs.len(), CATALOG_OVERRIDES.len());
    }
}
