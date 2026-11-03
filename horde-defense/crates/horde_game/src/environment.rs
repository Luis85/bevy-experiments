//! Typed optional process configuration. Empty Windows environment values are unset.
use std::{ffi::OsString, path::PathBuf};

pub fn string(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}
pub fn path(name: &str) -> Option<PathBuf> {
    nonempty_path(std::env::var_os(name))
}
fn nonempty_path(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}
fn parse_flag(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}
pub fn flag(name: &str) -> bool {
    parse_flag(string(name).as_deref())
}

/// Explain every once-loaded authored source in the startup diagnostic console.
pub fn catalog_sources() -> Vec<(&'static str, String)> {
    [
        "LASTLIGHT_CONTENT",
        "LASTLIGHT_WORLD",
        "LASTLIGHT_SKILLS",
        "LASTLIGHT_ENEMIES",
        "LASTLIGHT_ORDNANCE",
        "LASTLIGHT_PROGRESSION",
        "LASTLIGHT_QUALITY",
        "LASTLIGHT_BALANCE",
    ]
    .into_iter()
    .map(|name| {
        (
            name,
            path(name).map_or_else(
                || "bundled defaults".to_owned(),
                |p| p.display().to_string(),
            ),
        )
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_false_opt_ins_never_enable_practice_or_performance() {
        for value in [
            None,
            Some(""),
            Some(" "),
            Some("0"),
            Some("false"),
            Some("off"),
        ] {
            assert!(!parse_flag(value));
        }
        for value in [Some("1"), Some(" true "), Some("YES"), Some("on")] {
            assert!(parse_flag(value));
        }
    }
    #[test]
    fn empty_report_paths_are_absent_not_current_directory() {
        assert!(nonempty_path(None).is_none());
        assert!(nonempty_path(Some(OsString::new())).is_none());
        assert_eq!(
            nonempty_path(Some(OsString::from("target/report.json"))),
            Some(PathBuf::from("target/report.json"))
        );
    }
}
