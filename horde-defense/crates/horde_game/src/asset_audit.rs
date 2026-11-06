//! Asset provenance and orphan audit. Test-only; it reads files, not game state.
//!
//! Every shipped `.wav` and `.png` under `assets/` is loaded at runtime and has
//! exactly one provenance record whose `sha256` matches the file. Everything
//! moved to `archive/` (production-pass decision 10: not shipped, nothing
//! deleted) keeps its record, its hash and the `assets/` path it came from, at
//! the mirrored place under `archive/`. The asset index in `assets/CREDITS.md`
//! is rendered from the same manifests by the ignored writer
//! [`regenerate_credits_index`] and guarded by [`credits_index_matches_provenance`].
//!
//! The runtime sets come from the code that loads the files: the egui art slots
//! ([`crate::visual_assets::PATHS`], which also hold the enemy-visual sheets),
//! the player-model sources and [`crate::audio::runtime_wav_paths`]. A commit
//! that adds or retires an asset extends those, never a list in this module.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

/// The two shipped audio manifests, one writer each: the Python family
/// (`scripts/generate_atmosphere_audio.py`) and the Rust survival exporter.
const AUDIO_MANIFESTS: [&str; 2] = [
    "assets/audio/PROVENANCE.json",
    "assets/audio/SURVIVAL_PROVENANCE.json",
];
/// The shipped art manifests. The root one keeps no shipped entry since the
/// archive; its six sheets are recorded in `archive/art/PROVENANCE.json`.
const ART_MANIFESTS: [&str; 2] = [
    "assets/art/PROVENANCE.json",
    "assets/art/pixel/PROVENANCE.json",
];
/// The archive manifests, entries keyed relative to their own folder. The
/// survival one holds the retired `Cue::NpcShot` variants.
const ARCHIVE_MANIFESTS: [&str; 3] = [
    "archive/art/PROVENANCE.json",
    "archive/audio/PROVENANCE.json",
    "archive/audio/SURVIVAL_PROVENANCE.json",
];
/// The Python manifest of archived audio, written by the atmosphere script.
const PYTHON_ARCHIVE_MANIFEST: &str = "archive/audio/PROVENANCE.json";
/// The user-approved legacy WAVs (production-pass plan 2.3), now archived.
const LEGACY_WAVS: [&str; 12] = [
    "armor.wav",
    "boss.wav",
    "damage.wav",
    "incoming.wav",
    "melee.wav",
    "npc_shot.wav",
    "ready.wav",
    "reload.wav",
    "shot.wav",
    "structure.wav",
    "trap.wav",
    "turret_shot.wav",
];
/// The only writers a Python manifest record may name.
const PYTHON_GENERATORS: [&str; 2] = [
    "scripts/generate_audio.py",
    "scripts/generate_atmosphere_audio.py",
];
const CREDITS: &str = "assets/CREDITS.md";
const INDEX_BEGIN: &str = "<!-- asset-index:begin -->";
const INDEX_END: &str = "<!-- asset-index:end -->";

/// The `horde-defense/` workspace folder.
pub(crate) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(relative: &str) -> Value {
    let path = workspace_root().join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {relative}: {error}"))
}

/// The SHA-256 of a workspace file as 64 lowercase hex characters, read and
/// hashed once per test process.
pub(crate) fn file_sha256(relative: &str) -> String {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hash) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(relative)
    {
        return hash.clone();
    }
    let bytes = std::fs::read(workspace_root().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"));
    let hash = horde_core::sha256::hex(&bytes);
    cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(relative.to_owned(), hash.clone());
    hash
}

/// Every file below the workspace folder `directory`, as sorted `/`-separated
/// workspace paths. A missing folder has no files.
fn files_under(directory: &str) -> Vec<String> {
    fn walk(root: &Path, directory: &Path, found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                let relative = path.strip_prefix(root).expect("below the root");
                let parts: Vec<String> = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect();
                found.push(parts.join("/"));
            }
        }
    }
    let root = workspace_root();
    let mut found = Vec::new();
    walk(&root, &root.join(directory), &mut found);
    found.sort();
    found
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

/// Every shipped file with `extension` (`assets/...` workspace paths).
pub(crate) fn shipped_files(extension: &str) -> BTreeSet<String> {
    files_under("assets")
        .into_iter()
        .filter(|path| has_extension(path, extension))
        .collect()
}

/// One provenance record: the workspace path it describes, the manifest that
/// holds it and the record itself.
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub path: String,
    pub manifest: String,
    pub value: Value,
    /// The record's `generator`, else the manifest's.
    pub generator: String,
}
impl Record {
    fn text(&self, key: &str) -> Option<&str> {
        self.value.get(key).and_then(Value::as_str)
    }
}

/// The records of one manifest: its `assets` or `files` array, each entry's
/// `file` resolved against the manifest's folder.
fn manifest_records(manifest: &str) -> Vec<Record> {
    let json = read_json(manifest);
    let folder = manifest.rsplit_once('/').map_or("", |(folder, _)| folder);
    let entries = json
        .get("assets")
        .or_else(|| json.get("files"))
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{manifest} has no assets or files array"));
    let fallback = json.get("generator").and_then(Value::as_str).unwrap_or("");
    entries
        .iter()
        .map(|entry| {
            let file = entry
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{manifest}: an entry has no file"));
            Record {
                path: format!("{folder}/{file}"),
                manifest: manifest.to_owned(),
                value: entry.clone(),
                generator: entry
                    .get("generator")
                    .and_then(Value::as_str)
                    .unwrap_or(fallback)
                    .to_owned(),
            }
        })
        .collect()
}

fn records(manifests: &[&str]) -> Vec<Record> {
    manifests
        .iter()
        .flat_map(|manifest| manifest_records(manifest))
        .collect()
}

/// Records of the shipped audio manifests.
pub(crate) fn audio_manifests() -> Vec<Record> {
    records(&AUDIO_MANIFESTS)
}

/// Records of the shipped art manifests.
pub(crate) fn art_manifests() -> Vec<Record> {
    records(&ART_MANIFESTS)
}

/// Records of the archive manifests.
pub(crate) fn archive_manifest() -> Vec<Record> {
    records(&ARCHIVE_MANIFESTS)
}

fn by_path(records: Vec<Record>) -> BTreeMap<String, Vec<Record>> {
    let mut map: BTreeMap<String, Vec<Record>> = BTreeMap::new();
    for record in records {
        map.entry(record.path.clone()).or_default().push(record);
    }
    map
}

/// Each shipped file has exactly one record and its `sha256` matches the file.
fn check_provenance(shipped: &BTreeSet<String>, records: Vec<Record>) -> Vec<String> {
    let records = by_path(records);
    let mut problems = Vec::new();
    for path in shipped {
        match records.get(path).map(Vec::as_slice) {
            None | Some([]) => problems.push(format!("{path}: no provenance record")),
            Some([record]) => match record.text("sha256") {
                None => problems.push(format!("{path}: {} records no sha256", record.manifest)),
                Some(hash) if hash != file_sha256(path) => {
                    problems.push(format!("{path}: sha256 differs from {}", record.manifest));
                }
                Some(_) => {}
            },
            Some(several) => problems.push(format!("{path}: {} records", several.len())),
        }
    }
    problems
}

/// The PNGs the runtime loads: the egui art slots and every player-model source.
/// The enemy-visual sheets are art slots; each sheet named in
/// `assets/art/enemy_visuals.json` must be one.
pub(crate) fn runtime_art_paths() -> BTreeSet<String> {
    let slots = crate::visual_assets::PATHS;
    let visuals = read_json("assets/art/enemy_visuals.json");
    for sheet in visuals["sheets"].as_object().expect("sheets").keys() {
        let prefix = format!("art/pixel/{sheet}-");
        assert!(
            slots.iter().any(|slot| slot.starts_with(&prefix)),
            "enemy-visual sheet {sheet} is not an art slot"
        );
    }
    slots
        .iter()
        .map(|slot| format!("assets/{slot}"))
        .chain(
            player_model_sources()
                .into_iter()
                .map(|(path, _)| format!("assets/{path}")),
        )
        .collect()
}

/// Every `(path, png_sha256)` source of the player-model manifest's model and
/// skin files, paths relative to `assets/`.
pub(crate) fn player_model_sources() -> BTreeSet<(String, String)> {
    fn walk(value: &Value, found: &mut BTreeSet<(String, String)>) {
        match value {
            Value::Object(map) => {
                if let (Some(Value::String(path)), Some(Value::String(hash))) =
                    (map.get("path"), map.get("png_sha256"))
                {
                    found.insert((path.clone(), hash.clone()));
                }
                map.values().for_each(|child| walk(child, found));
            }
            Value::Array(items) => items.iter().for_each(|child| walk(child, found)),
            _ => {}
        }
    }
    let manifest = read_json("assets/player_models/manifest.json");
    let mut found = BTreeSet::new();
    for key in ["models", "skins"] {
        for file in manifest[key].as_array().expect("file list") {
            let file = file.as_str().expect("file name");
            walk(
                &read_json(&format!("assets/player_models/{file}")),
                &mut found,
            );
        }
    }
    assert!(
        !found.is_empty(),
        "the player-model manifest names no source"
    );
    found
}

/// One `| file | generator | manifest | sha256 |` row per shipped `.png` and
/// `.wav`, sorted by path, with paths relative to `assets/` as CREDITS uses them.
pub(crate) fn render_credits_index() -> String {
    let records = by_path(
        art_manifests()
            .into_iter()
            .chain(audio_manifests())
            .collect(),
    );
    let mut text = String::from(INDEX_BEGIN);
    text.push_str(
        "\n<!-- Generated from the provenance manifests by \
         asset_audit::regenerate_credits_index; do not edit by hand. -->\n\
         | File | Generator | Provenance | SHA-256 |\n|---|---|---|---|\n",
    );
    let shipped: BTreeSet<String> = shipped_files("png")
        .into_iter()
        .chain(shipped_files("wav"))
        .collect();
    for path in &shipped {
        let record = match records.get(path).map(Vec::as_slice) {
            Some([record]) => record,
            _ => panic!("{path}: needs exactly one provenance record"),
        };
        let mut generator = record.generator.clone();
        if let Some(seed) = record.value.get("seed").and_then(Value::as_u64) {
            generator.push_str(&format!(", seed {seed}"));
        }
        let hash = record
            .text("sha256")
            .unwrap_or_else(|| panic!("{path}: no sha256"));
        text.push_str(&format!(
            "| `{}` | {generator} | `{}` | `{}` |\n",
            path.strip_prefix("assets/").expect("shipped"),
            record.manifest.strip_prefix("assets/").expect("shipped"),
            &hash[..12]
        ));
    }
    text.push_str(INDEX_END);
    text
}

/// The asset-index block of `assets/CREDITS.md`, markers included.
fn credits_index_block(credits: &str) -> &str {
    let begin = credits
        .find(INDEX_BEGIN)
        .expect("CREDITS has no asset-index begin marker");
    let end = credits
        .find(INDEX_END)
        .expect("CREDITS has no asset-index end marker");
    assert!(begin < end, "asset-index markers out of order");
    &credits[begin..end + INDEX_END.len()]
}

fn read_credits() -> String {
    std::fs::read_to_string(workspace_root().join(CREDITS))
        .expect("read assets/CREDITS.md")
        .replace("\r\n", "\n")
}

/// Every `` `token` `` in `text`.
fn backticked(text: &str) -> Vec<&str> {
    text.split('`').skip(1).step_by(2).collect()
}

#[test]
fn every_shipped_wav_is_loaded_and_matches_provenance() {
    let shipped = shipped_files("wav");
    let runtime: BTreeSet<String> = crate::audio::runtime_wav_paths()
        .into_iter()
        .map(|path| format!("assets/{path}"))
        .collect();
    let orphans: Vec<_> = shipped.difference(&runtime).collect();
    let missing: Vec<_> = runtime.difference(&shipped).collect();
    assert!(
        orphans.is_empty() && missing.is_empty(),
        "{} orphan WAVs {orphans:?}; {} runtime WAVs missing {missing:?}",
        orphans.len(),
        missing.len()
    );
    assert_eq!(shipped.len(), 75, "shipped WAV count");
    let problems = check_provenance(&shipped, audio_manifests());
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn shipped_art_has_provenance_with_matching_sha256() {
    let problems = check_provenance(&shipped_files("png"), art_manifests());
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn shipped_art_is_referenced_at_runtime() {
    let shipped = shipped_files("png");
    let runtime = runtime_art_paths();
    let orphans: Vec<_> = shipped.difference(&runtime).collect();
    let missing: Vec<_> = runtime.difference(&shipped).collect();
    assert!(
        orphans.is_empty() && missing.is_empty(),
        "{} unreferenced PNGs {orphans:?}; {} runtime PNGs missing {missing:?}",
        orphans.len(),
        missing.len()
    );
}

#[test]
fn archived_files_keep_matching_provenance() {
    let records = archive_manifest();
    assert_eq!(records.len(), 27, "archived file count");
    let mut problems = Vec::new();
    for record in &records {
        let path = workspace_root().join(&record.path);
        let Ok(metadata) = std::fs::metadata(&path) else {
            problems.push(format!("{}: missing", record.path));
            continue;
        };
        if record.text("sha256") != Some(file_sha256(&record.path).as_str()) {
            problems.push(format!(
                "{}: sha256 differs from {}",
                record.path, record.manifest
            ));
        }
        if record.value.get("bytes").and_then(Value::as_u64) != Some(metadata.len()) {
            problems.push(format!(
                "{}: bytes differ from {}",
                record.path, record.manifest
            ));
        }
    }
    let recorded: BTreeSet<&str> = records.iter().map(|record| record.path.as_str()).collect();
    for path in files_under("archive") {
        if (has_extension(&path, "png") || has_extension(&path, "wav"))
            && !recorded.contains(path.as_str())
        {
            problems.push(format!("{path}: archived without a record"));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn archive_paths_mirror_archived_from() {
    let mut problems = Vec::new();
    for record in archive_manifest() {
        let mirrored = format!(
            "assets/{}",
            record.path.strip_prefix("archive/").expect("archive path")
        );
        if record.text("archived_from") != Some(mirrored.as_str()) {
            problems.push(format!("{}: archived_from is not {mirrored}", record.path));
        }
        if workspace_root().join(&mirrored).exists() {
            problems.push(format!("{mirrored}: still shipped beside its archive copy"));
        }
        let date = record.text("archived").unwrap_or("");
        let iso = date.len() == 10
            && date.char_indices().all(|(index, character)| match index {
                4 | 7 => character == '-',
                _ => character.is_ascii_digit(),
            });
        if !iso {
            problems.push(format!(
                "{}: archived {date:?} is not YYYY-MM-DD",
                record.path
            ));
        }
        if record
            .text("reason")
            .is_none_or(|reason| reason.trim().is_empty())
        {
            problems.push(format!("{}: no reason", record.path));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn player_model_png_sha256_matches_file() {
    for (path, hash) in player_model_sources() {
        assert_eq!(
            file_sha256(&format!("assets/{path}")),
            hash,
            "{path}: png_sha256 differs from the file"
        );
    }
}

#[test]
fn every_manifest_entry_exists() {
    let manifests: Vec<String> = files_under("assets")
        .into_iter()
        .chain(files_under("archive"))
        .filter(|path| path.ends_with("PROVENANCE.json"))
        .collect();
    for required in AUDIO_MANIFESTS
        .iter()
        .chain(&ART_MANIFESTS)
        .chain(&ARCHIVE_MANIFESTS)
    {
        assert!(
            manifests.iter().any(|path| path == required),
            "{required} is missing"
        );
    }
    let mut problems = Vec::new();
    for manifest in &manifests {
        for record in manifest_records(manifest) {
            if !workspace_root().join(&record.path).is_file() {
                problems.push(format!("{manifest}: {} does not exist", record.path));
            }
        }
        // Preserved originals name clips beside the manifest, by stem.
        let folder = manifest.rsplit_once('/').map_or("", |(folder, _)| folder);
        if let Some(originals) = read_json(manifest)
            .get("preserved_originals")
            .and_then(Value::as_object)
        {
            for (stem, hash) in originals {
                let path = format!("{folder}/{stem}.wav");
                if !workspace_root().join(&path).is_file() {
                    problems.push(format!(
                        "{manifest}: preserved original {path} does not exist"
                    ));
                } else if hash.as_str() != Some(file_sha256(&path).as_str()) {
                    problems.push(format!("{manifest}: preserved original {path} changed"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn credits_index_matches_provenance() {
    let credits = read_credits();
    assert_eq!(
        credits_index_block(&credits),
        render_credits_index(),
        "assets/CREDITS.md asset index is stale; run the ignored \
         asset_audit::regenerate_credits_index"
    );
}

#[test]
fn credits_index_lists_every_shipped_asset_exactly_once() {
    let credits = read_credits();
    let mut listed: BTreeMap<String, usize> = BTreeMap::new();
    for line in credits_index_block(&credits).lines() {
        if let Some(path) = line
            .strip_prefix("| `")
            .and_then(|rest| rest.split('`').next())
        {
            *listed.entry(format!("assets/{path}")).or_default() += 1;
        }
    }
    let shipped: BTreeSet<String> = shipped_files("png")
        .into_iter()
        .chain(shipped_files("wav"))
        .collect();
    let twice: Vec<_> = listed.iter().filter(|(_, count)| **count > 1).collect();
    let unlisted: Vec<_> = shipped
        .iter()
        .filter(|path| !listed.contains_key(*path))
        .collect();
    let stale: Vec<_> = listed
        .keys()
        .filter(|path| !shipped.contains(*path))
        .collect();
    assert!(
        twice.is_empty() && unlisted.is_empty() && stale.is_empty(),
        "listed twice {twice:?}; unlisted {unlisted:?}; listed but not shipped {stale:?}"
    );
}

#[test]
fn credits_never_lists_an_archived_path_as_shipped() {
    let credits = read_credits();
    let archived: BTreeSet<String> = archive_manifest()
        .into_iter()
        .filter_map(|record| record.text("archived_from").map(str::to_owned))
        .collect();
    let mut problems = Vec::new();
    for token in backticked(&credits) {
        let asset = has_extension(token, "png") || has_extension(token, "wav");
        let manifest =
            token.ends_with(".json") && (token.starts_with("art/") || token.starts_with("audio/"));
        if token.contains('*') || !(asset || manifest || token.starts_with("archive/")) {
            continue;
        }
        let resolved = if token.starts_with("archive/") {
            token.to_owned()
        } else {
            format!("assets/{token}")
        };
        if archived.contains(&resolved) {
            problems.push(format!("`{token}` is archived; cite it as archive/..."));
        } else if !workspace_root().join(&resolved).exists() {
            problems.push(format!("`{token}` does not resolve to {resolved}"));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn python_manifests_name_only_owned_files() {
    let file_names = |records: &[Record]| -> BTreeSet<String> {
        records
            .iter()
            .map(|record| record.text("file").expect("file").to_owned())
            .collect()
    };
    let shipped = manifest_records(AUDIO_MANIFESTS[0]);
    let beds: BTreeSet<String> = crate::atmosphere_audio::AmbienceChannel::ALL
        .iter()
        .map(|channel| {
            channel
                .path()
                .strip_prefix("audio/")
                .expect("audio")
                .to_owned()
        })
        .collect();
    assert_eq!(file_names(&shipped), beds, "assets/audio/PROVENANCE.json");
    let archived = manifest_records(PYTHON_ARCHIVE_MANIFEST);
    let legacy: BTreeSet<String> = LEGACY_WAVS.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(file_names(&archived), legacy, "{PYTHON_ARCHIVE_MANIFEST}");
    for record in shipped.iter().chain(&archived) {
        assert!(
            PYTHON_GENERATORS.contains(&record.generator.as_str()),
            "{}: generator {}",
            record.path,
            record.generator
        );
        // The named generator lists the clip among its owned files.
        let source = std::fs::read_to_string(workspace_root().join(&record.generator))
            .expect("read generator");
        let stem = record.text("file").expect("file").trim_end_matches(".wav");
        assert!(
            source.contains(&format!("\"{stem}\"")) || source.contains(&format!("'{stem}'")),
            "{} does not own {stem}",
            record.generator
        );
    }
    let originals = |manifest: &str| -> BTreeSet<String> {
        read_json(manifest)["preserved_originals"]
            .as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    };
    assert_eq!(
        originals(AUDIO_MANIFESTS[0]),
        BTreeSet::from(["ambience".to_owned()])
    );
    assert_eq!(originals(PYTHON_ARCHIVE_MANIFEST).len(), 8);
}

/// Writer, not a check: renders the asset index into `assets/CREDITS.md`.
#[test]
#[ignore = "Writes assets/CREDITS.md; run after a provenance change"]
fn regenerate_credits_index() {
    let credits = read_credits();
    let block = credits_index_block(&credits).to_owned();
    let updated = credits.replacen(&block, &render_credits_index(), 1);
    std::fs::write(workspace_root().join(CREDITS), updated).expect("write assets/CREDITS.md");
}
