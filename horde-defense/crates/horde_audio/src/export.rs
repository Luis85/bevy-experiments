//! Exporter v2: the shipped survival library and its schema-2 provenance.
//!
//! [`library`] renders and masters every variant of every shipped bank;
//! [`manifest`] describes the result as `assets/audio/SURVIVAL_PROVENANCE.json`
//! (ruling S11: the Rust family's own manifest, schema 2, one writer).
//! `examples/export_audio.rs` is the writer: it checks by default and writes
//! only with `--write`. Nothing here runs in the game.
//!
//! This crate has no SHA-256 of its own (ruling S7), so the functions that
//! need one take it as `hash`; the example and the tests pass
//! `horde_core::sha256::hex`.

use crate::{
    bank::{BankId, seed},
    meter,
    synth::{
        family, legacy,
        master::{self, CEILING, LOOKAHEAD_SECONDS, MAX_GAIN_REDUCTION_DB, RELEASE_SECONDS},
    },
    wav,
};
use std::path::Path;

/// The exporter, as the provenance names it.
pub const GENERATOR: &str = "crates/horde_audio/examples/export_audio.rs";
/// Every source file that decides a sample or a provenance number, sorted,
/// relative to the workspace folder: the list is closed over the crate files
/// they use (`bank.rs` holds every variant's seed and file name, `cue.rs` the
/// legacy ordinals). `generator_sha256` covers exactly these; after editing
/// one, run the exporter with `--write`.
pub const GENERATOR_SOURCES: [&str; 9] = [
    "crates/horde_audio/examples/export_audio.rs",
    "crates/horde_audio/src/bank.rs",
    "crates/horde_audio/src/cue.rs",
    "crates/horde_audio/src/export.rs",
    "crates/horde_audio/src/meter.rs",
    "crates/horde_audio/src/synth/family.rs",
    "crates/horde_audio/src/synth/legacy.rs",
    "crates/horde_audio/src/synth/master.rs",
    "crates/horde_audio/src/wav.rs",
];
/// The shipped manifest, relative to the workspace folder.
pub const MANIFEST: &str = "assets/audio/SURVIVAL_PROVENANCE.json";
/// The verbatim copy of the v1 manifest the exporter replaced.
pub const V1_MANIFEST: &str = "archive/audio/SURVIVAL_PROVENANCE.v1.json";

/// One shipped file.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub bank: BankId,
    pub variant: u64,
    /// The file name inside `assets/audio/`.
    pub file: String,
    pub seed: u64,
    /// The loudness the bank is mastered to: -21 LUFS momentary-max, or lower
    /// for a peak-bound bank (`synth::master`).
    pub target_mmax_lufs: f64,
    /// The mastered mono samples, on the PCM16 grid.
    pub samples: Vec<f32>,
    pub limiter_gr_db: f32,
}
impl Clip {
    /// The file's bytes.
    pub fn bytes(&self) -> Vec<u8> {
        wav::pcm16_bytes(legacy::RATE, &self.samples)
    }
}

/// The unmastered synthesis of one variant: what the mastering stage reads.
/// For a v1 family's variants 0-2 it is the v1 library, sample for sample.
pub fn raw(bank: BankId, variant: u64) -> Vec<f32> {
    match bank {
        BankId::Legacy(cue) => legacy::synthesize(cue, variant),
        _ => family::synthesize(bank, variant),
    }
}

/// One bank mastered: every variant at the bank's target.
pub fn bank_clips(bank: BankId) -> Vec<Clip> {
    let raws: Vec<Vec<f32>> = (0..bank.variants())
        .map(|variant| raw(bank, variant))
        .collect();
    let target = master::bank_target(legacy::RATE, &raws);
    raws.iter()
        .zip(0..)
        .map(|(raw, variant)| {
            let mastered = master::master(legacy::RATE, raw, target);
            let path = bank.variant_path(variant);
            Clip {
                bank,
                variant,
                file: path.strip_prefix("audio/").unwrap_or(&path).to_owned(),
                seed: seed(bank, variant),
                target_mmax_lufs: target,
                samples: mastered.samples,
                limiter_gr_db: mastered.limiter_gr_db,
            }
        })
        .collect()
}

/// Every shipped file, in bank-ordinal then variant order.
pub fn library() -> Vec<Clip> {
    BankId::shipped().into_iter().flat_map(bank_clips).collect()
}

/// The bytes `generator_sha256` hashes: each of `sources` as its path, a line
/// feed, and its text with CRLF folded to LF, so a CRLF checkout hashes the
/// same. `sources` are `(path, contents)` in [`GENERATOR_SOURCES`] order.
pub fn generator_digest_input(sources: &[(&str, String)]) -> Vec<u8> {
    let mut input = Vec::new();
    for (path, text) in sources {
        input.extend_from_slice(path.as_bytes());
        input.push(b'\n');
        input.extend_from_slice(text.replace("\r\n", "\n").as_bytes());
        input.push(b'\n');
    }
    input
}

/// [`generator_digest_input`] of the generator sources below `root`.
pub fn read_generator_digest_input(root: &Path) -> std::io::Result<Vec<u8>> {
    let mut sources = Vec::new();
    for path in GENERATOR_SOURCES {
        sources.push((path, std::fs::read_to_string(root.join(path))?));
    }
    Ok(generator_digest_input(&sources))
}

/// The schema-2 manifest text for `clips` (LF line ends, trailing newline).
pub fn manifest(clips: &[Clip], generator_sha256: &str, hash: &dyn Fn(&[u8]) -> String) -> String {
    let mut text = String::from("{\n  \"schema\": 2,\n");
    text.push_str("  \"license\": \"MIT (repository original assets)\",\n");
    text.push_str(
        "  \"policy\": {\"recordings\": false, \"third_party\": false, \"ai_audio\": false, \"music\": false},\n",
    );
    text.push_str(
        "  \"inputs\": \"Original deterministic Rust synthesis; no recordings, third-party samples or model-generated audio\",\n",
    );
    text.push_str(&format!("  \"generator\": \"{GENERATOR}\",\n"));
    text.push_str("  \"generator_sources\": [\n");
    for (index, source) in GENERATOR_SOURCES.iter().enumerate() {
        let comma = if index + 1 < GENERATOR_SOURCES.len() {
            ","
        } else {
            ""
        };
        text.push_str(&format!("    \"{source}\"{comma}\n"));
    }
    text.push_str("  ],\n");
    text.push_str(&format!(
        "  \"generator_sha256\": \"{generator_sha256}\",\n"
    ));
    text.push_str(
        "  \"generator_sha256_note\": \"sha256 over generator_sources in order: each path, LF, the file with CRLF folded to LF, LF\",\n",
    );
    text.push_str("  \"format\": \"44100 Hz, mono, signed PCM16 little endian WAV\",\n");
    text.push_str(&format!(
        "  \"mastering\": {{\"target_mmax_lufs\": {:.1}, \"tolerance_lu\": {:.1}, \"true_peak_ceiling\": {:.2}, \"lookahead_ms\": {:.1}, \"release_ms\": {:.1}, \"max_limiter_gr_db\": {:.1}}},\n",
        master::TARGET_MMAX_LUFS,
        master::TOLERANCE_LU,
        CEILING,
        LOOKAHEAD_SECONDS * 1000.0,
        RELEASE_SECONDS * 1000.0,
        MAX_GAIN_REDUCTION_DB
    ));
    text.push_str(
        "  \"mastering_note\": \"A file's target_mmax_lufs is the mastering target of its bank. It is below -21 only for a peak-bound v1 family: its frozen synthesis cannot reach -21 LUFS within the limiter bound, so the bank is mastered to the loudest half-LU step all its variants reach.\",\n",
    );
    text.push_str(&format!("  \"v1\": \"{V1_MANIFEST}\",\n"));
    text.push_str("  \"files\": [\n");
    for (index, clip) in clips.iter().enumerate() {
        let loudness = meter::measure(legacy::RATE, &[&clip.samples]);
        let comma = if index + 1 < clips.len() { "," } else { "" };
        text.push_str(&format!(
            "    {{\"file\": \"{}\", \"bank\": \"{}\", \"variant\": {}, \"generator\": \"{GENERATOR}\", \"generator_sha256\": \"{generator_sha256}\", \"seed\": {}, \"sample_rate\": {}, \"channels\": 1, \"frames\": {}, \"sha256\": \"{}\", \"peak\": {:.6}, \"true_peak\": {:.6}, \"mmax_lufs\": {:.2}, \"target_mmax_lufs\": {:.1}, \"limiter_gr_db\": {:.2}, \"seam_limit\": null}}{comma}\n",
            clip.file,
            clip.bank.name(),
            clip.variant,
            clip.seed,
            legacy::RATE,
            clip.samples.len(),
            hash(&clip.bytes()),
            loudness.sample_peak,
            loudness.true_peak,
            loudness.momentary_max,
            clip.target_mmax_lufs,
            clip.limiter_gr_db,
        ));
    }
    text.push_str("  ]\n}\n");
    text
}

/// What differs between `clips` plus their manifest and the files below
/// `root`: one line per problem, empty when the export is up to date. It
/// reads only; `--check` prints this.
pub fn check(root: &Path, clips: &[Clip], hash: &dyn Fn(&[u8]) -> String) -> Vec<String> {
    let mut problems = Vec::new();
    for clip in clips {
        let path = root.join("assets/audio").join(&clip.file);
        match std::fs::read(&path) {
            Ok(bytes) if bytes == clip.bytes() => {}
            Ok(_) => problems.push(format!("{}: differs from the export", clip.file)),
            Err(error) => problems.push(format!("{}: {error}", clip.file)),
        }
    }
    // A survival file the exporter no longer renders is an orphan.
    if let Ok(entries) = std::fs::read_dir(root.join("assets/audio")) {
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| name.starts_with("survival_") && name.ends_with(".wav"))
            .collect();
        names.sort();
        for name in names {
            if !clips.iter().any(|clip| clip.file == name) {
                problems.push(format!("{name}: not an exported file"));
            }
        }
    }
    match read_generator_digest_input(root) {
        Ok(input) => {
            let expected = manifest(clips, &hash(&input), hash);
            match std::fs::read_to_string(root.join(MANIFEST)) {
                Ok(text) if text.replace("\r\n", "\n") == expected => {}
                Ok(_) => problems.push(format!("{MANIFEST}: differs from the export")),
                Err(error) => problems.push(format!("{MANIFEST}: {error}")),
            }
        }
        Err(error) => problems.push(format!("generator sources: {error}")),
    }
    if !root.join(V1_MANIFEST).is_file() {
        problems.push(format!("{V1_MANIFEST}: missing"));
    }
    problems
}

/// The audition reel: every bank's first variant with a third of a second of
/// silence after each, and its `cue,start_seconds,duration_seconds` index.
/// The reel is written below `target/audio-audition/` and never committed.
pub fn audition_reel(clips: &[Clip]) -> (Vec<f32>, String) {
    let mut reel = Vec::new();
    let mut index = String::from("bank,start_seconds,duration_seconds\n");
    for clip in clips.iter().filter(|clip| clip.variant == 0) {
        index.push_str(&format!(
            "{},{:.3},{:.3}\n",
            clip.bank.name(),
            reel.len() as f32 / legacy::RATE as f32,
            clip.samples.len() as f32 / legacy::RATE as f32
        ));
        reel.extend_from_slice(&clip.samples);
        reel.resize(reel.len() + legacy::RATE as usize / 3, 0.0);
    }
    (reel, index)
}
