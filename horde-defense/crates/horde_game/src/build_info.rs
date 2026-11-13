//! Build stamp: the workspace version plus the commit the playtest package was
//! built from. Only `scripts/package_playtest.ps1` sets
//! `LASTLIGHT_BUILD_COMMIT` (at compile time); every other build is a dev
//! build. Shown on the menu footer, in crash reports, in the startup log and
//! by `--version`. Never written into a save.

/// Longest accepted commit id, in hex digits.
const MAX_COMMIT_HEX: usize = 20;
const DIRTY: &str = "+dirty";

/// `"v0.1.0 (dev build)"`, `"v0.1.0 (abc1234)"` or `"v0.1.0 (abc1234+dirty)"`.
/// A commit counts only when it is 1 to 20 ASCII lowercase hex digits,
/// optionally followed by `+dirty`; anything else reads as a dev build.
pub fn stamp_from(version: &str, commit: Option<&str>) -> String {
    match commit.filter(|commit| is_commit(commit)) {
        Some(commit) => format!("v{version} ({commit})"),
        None => format!("v{version} (dev build)"),
    }
}

fn is_commit(commit: &str) -> bool {
    let hex = commit.strip_suffix(DIRTY).unwrap_or(commit);
    (1..=MAX_COMMIT_HEX).contains(&hex.len())
        && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The stamp of this executable.
pub fn stamp() -> String {
    stamp_from(
        env!("CARGO_PKG_VERSION"),
        option_env!("LASTLIGHT_BUILD_COMMIT"),
    )
}

/// What `--version` prints.
pub fn version_line() -> String {
    format!("Lastlight: Horde Defense {}", stamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_formats_dev_and_commit_builds() {
        assert_eq!(stamp_from("0.1.0", None), "v0.1.0 (dev build)");
        assert_eq!(stamp_from("0.1.0", Some("abc1234")), "v0.1.0 (abc1234)");
        assert_eq!(
            stamp_from("0.1.0", Some("abc1234+dirty")),
            "v0.1.0 (abc1234+dirty)"
        );
        let twenty = "0123456789abcdef0123";
        assert_eq!(
            stamp_from("1.2.3", Some(twenty)),
            format!("v1.2.3 ({twenty})")
        );
        // Anything that is not a short lowercase hex id is a dev build.
        for bad in [
            "",
            "+dirty",
            "ABC1234",
            "abc123g",
            "abc 123",
            "abc1234+DIRTY",
            "abc1234+dirty+dirty",
            "abc1234-dirty",
            "0123456789abcdef01234",
            "0123456789abcdef01234+dirty",
            "abc1234\n",
            "(abc1234)",
        ] {
            assert_eq!(
                stamp_from("0.1.0", Some(bad)),
                "v0.1.0 (dev build)",
                "{bad:?}"
            );
        }
    }

    /// The scripts stay ASCII (PowerShell 5.1 reads BOM-less files as ANSI),
    /// so the stamp they compare against is ASCII too.
    #[test]
    fn stamp_of_this_build_is_ascii_and_carries_the_package_version() {
        let stamp = stamp();
        assert!(stamp.is_ascii(), "{stamp}");
        assert!(
            stamp.starts_with(&format!("v{} (", env!("CARGO_PKG_VERSION"))),
            "{stamp}"
        );
        assert!(stamp.ends_with(')'), "{stamp}");
        assert_eq!(version_line(), format!("Lastlight: Horde Defense {stamp}"));
    }
}
