//! `data-check`: load mods with the game's features and report what they contain.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fl_sim::BuildError;
use serde_json::json;

use crate::report::Report;

/// What a successful `data-check` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataCheckReport {
    /// Mod ids in load order.
    pub mods: Vec<String>,
    /// Concrete units.
    pub units: usize,
    /// Content kinds (by file extension) and how many items each has, sorted by kind.
    pub kinds: Vec<(&'static str, usize)>,
    /// Problems that do not stop loading.
    pub warnings: Vec<String>,
    /// The rules hash of the loaded content.
    pub rules_hash: u64,
}

/// A `'static` name for a kind extension; the built-in ones are interned, any other (a
/// feature's kind) is leaked once, which is fine for a short-lived tool.
fn intern(ext: &str) -> &'static str {
    ["faction", "map", "scenario", "terrain", "visual"]
        .into_iter()
        .find(|known| *known == ext)
        .unwrap_or_else(|| Box::leak(ext.to_owned().into_boxed_str()))
}

/// Load the mods in `dirs` with [`fl_catalog::features`] and summarise them.
///
/// # Errors
/// [`BuildError::Content`] carries every content error, not just the first.
pub fn data_check(dirs: &[PathBuf]) -> Result<DataCheckReport, BuildError> {
    let content = fl_sim::load_content(&fl_catalog::features(), dirs)?;
    let mut kinds: Vec<_> = content
        .string_tables()
        .tables
        .iter()
        .filter_map(|(table, names)| Some((intern(table.strip_prefix("content.")?), names.len())))
        .collect();
    kinds.sort_unstable();
    Ok(DataCheckReport {
        mods: content.mods().iter().map(|m| m.id.clone()).collect(),
        units: content.units().len(),
        kinds,
        warnings: content.warnings().to_vec(),
        rules_hash: content.rules_hash(),
    })
}

/// `data-check`: [`data_check`] as text (`rules hash: 0x…` last) or JSON (`{mods, units,
/// kinds, warnings, rules_hash}`). A failure is exit 1 with every error, as text or
/// `{"errors": [...]}`.
#[must_use]
pub fn data_check_report(dirs: &[PathBuf]) -> Report {
    let report = match data_check(dirs) {
        Ok(report) => report,
        Err(err) => {
            let errors: Vec<String> = match &err {
                BuildError::Content(errors) => errors.0.iter().map(ToString::to_string).collect(),
                other => vec![other.to_string()],
            };
            let mut text = "data-check failed:\n".to_owned();
            for error in &errors {
                text.push_str(error);
                text.push('\n');
            }
            return Report {
                json: json!({ "errors": errors }),
                text,
                code: 1,
            };
        }
    };
    let kinds: BTreeMap<_, _> = report.kinds.iter().copied().collect();
    let rules_hash = format!("0x{:016x}", report.rules_hash);
    let json = json!({
        "mods": report.mods,
        "units": report.units,
        "kinds": kinds,
        "warnings": report.warnings,
        "rules_hash": rules_hash,
    });
    let mut text = format!(
        "mods: {}\nunits: {}\n",
        report.mods.join(", "),
        report.units
    );
    for (kind, count) in &report.kinds {
        text.push_str(&format!("{kind}: {count}\n"));
    }
    for warning in &report.warnings {
        text.push_str(&format!("warning: {warning}\n"));
    }
    text.push_str(&format!("rules hash: {rules_hash}\n"));
    Report::ok(json, text)
}
