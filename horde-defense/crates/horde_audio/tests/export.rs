//! Exporter v2 (A6-05): the shipped survival library, its mastering and its
//! schema-2 provenance.

use horde_audio::{cue::Cue, mix::mix};
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A cue that can restart within 250 ms repeats audibly with three variants,
/// so its bank ships at least five (design-audio section 2, AUD-06).
#[test]
fn frequent_cues_have_at_least_five_variants() {
    let assets = workspace_root().join("assets");
    let mut frequent = 0;
    for cue in Cue::ALL.into_iter().filter(|cue| !cue.retired()) {
        if mix(cue).cooldown_ms > 250 {
            continue;
        }
        frequent += 1;
        let bank = cue.bank();
        assert!(
            bank.variants() >= 5,
            "{cue:?} restarts every {} ms but its bank has {} variants",
            mix(cue).cooldown_ms,
            bank.variants()
        );
        for variant in 0..bank.variants() {
            let path = bank.variant_path(variant);
            assert!(assets.join(&path).is_file(), "missing {path}");
        }
    }
    // Ready itself restarts every 400 ms; its bank is frequent through the
    // Confirm alias (250 ms).
    assert_eq!(
        frequent, 21,
        "16 families, the Kill and Confirm aliases, EnemyDeath, Block and ShellInsert"
    );
    assert_eq!(Cue::Confirm.bank(), Cue::Ready.bank());
}

// ----- the exported library -----

use horde_audio::{
    bank::{BankId, LEGACY_FAMILIES, UiFamily, seed},
    export::{self, Clip},
    meter,
    synth::{
        legacy::{self, RATE},
        master::{CEILING, MAX_GAIN_REDUCTION_DB, TARGET_MMAX_LUFS, TOLERANCE_LU},
    },
};
use serde_json::Value;
use std::sync::OnceLock;

fn hash(bytes: &[u8]) -> String {
    horde_core::sha256::hex(bytes)
}

/// The whole export, rendered once per test process.
fn library() -> &'static [Clip] {
    static LIBRARY: OnceLock<Vec<Clip>> = OnceLock::new();
    LIBRARY.get_or_init(export::library)
}

fn shipped_manifest() -> Value {
    let path = workspace_root().join(export::MANIFEST);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The v1 families that cannot reach -21 LUFS within the limiter bound, and
/// the target each is mastered to instead (`synth::master`). Their synthesis
/// is frozen (`raw_synthesis_matches_v1_library`), so the list only changes
/// with the mastering constants; the runtime mix calibration (A6-17) reads it.
const PEAK_BOUND: [(&str, f64); 16] = [
    ("pistol", -24.5),
    ("smg", -25.5),
    ("shotgun", -21.5),
    ("rifle", -24.0),
    ("marksman", -22.0),
    ("machinegun", -23.5),
    ("turret_shot", -24.0),
    ("impact", -24.0),
    ("armor", -23.0),
    ("reload", -21.5),
    ("molotov", -22.0),
    ("retreat", -22.0),
    ("wave", -22.5),
    ("damage", -23.0),
    ("ready", -24.5),
    ("incoming", -22.5),
];

#[test]
fn every_rust_family_masters_to_minus21_mmax_within_0_5_tp_le_0_80_gr_le_5db() {
    let library = library();
    assert_eq!(library.len(), 275, "103 v1-family files and 172 new ones");
    let mut problems = Vec::new();
    for clip in library {
        let loudness = meter::measure(RATE, &[&clip.samples]);
        if (loudness.momentary_max - clip.target_mmax_lufs).abs() > TOLERANCE_LU
            || loudness.true_peak > CEILING
            || clip.limiter_gr_db > MAX_GAIN_REDUCTION_DB
        {
            problems.push(format!(
                "{}: {:.2} LUFS mmax, true peak {:.4}, limiter {:.2} dB",
                clip.file, loudness.momentary_max, loudness.true_peak, clip.limiter_gr_db
            ));
        }
        assert!(clip.samples.iter().all(|sample| sample.is_finite()));
        assert_eq!(clip.samples[0], 0.0, "{}", clip.file);
        assert_eq!(*clip.samples.last().unwrap(), 0.0, "{}", clip.file);
    }
    assert!(problems.is_empty(), "{problems:#?}");
    // Every wave-6 family reaches -21 LUFS: a new sound that could not would
    // be reshaped. Only the listed v1 families sit lower.
    let mut bound: Vec<(String, f64)> = library
        .iter()
        .filter(|clip| clip.target_mmax_lufs != TARGET_MMAX_LUFS)
        .map(|clip| {
            assert!(matches!(clip.bank, BankId::Legacy(_)), "{}", clip.file);
            (clip.bank.name(), clip.target_mmax_lufs)
        })
        .collect();
    bound.dedup();
    let pinned: Vec<(String, f64)> = PEAK_BOUND
        .iter()
        .map(|(name, target)| ((*name).to_owned(), *target))
        .collect();
    assert_eq!(bound, pinned);
}

/// Mastering reads the v1 synthesis unchanged: what goes into the mastering
/// stage for variants 0-2 of a v1 family is the clip
/// `raw_synthesis_matches_v1_library` holds to the v1 hashes.
#[test]
fn legacy_v0_2_master_input_equals_v1_raw() {
    let v1: Value = serde_json::from_str(
        &std::fs::read_to_string(workspace_root().join(export::V1_MANIFEST)).unwrap(),
    )
    .unwrap();
    let v1 = v1["files"].as_array().unwrap();
    assert_eq!(v1.len(), 69, "the v1 manifest is archived verbatim");
    let mut checked = 0;
    for family in LEGACY_FAMILIES.into_iter().filter(|cue| !cue.retired()) {
        let bank = BankId::Legacy(family);
        for variant in 0..legacy::VARIANTS {
            let raw = export::raw(bank, variant);
            assert_eq!(raw, legacy::synthesize(family, variant));
            let file = format!("survival_{}_{variant}.wav", bank.name());
            let record = v1
                .iter()
                .find(|record| record["file"] == file.as_str())
                .unwrap_or_else(|| panic!("{file} has no v1 record"));
            assert_eq!(
                record["sha256"],
                hash(&horde_audio::wav::pcm16_bytes(RATE, &raw)).as_str(),
                "{file}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 69);
}

#[test]
fn new_variants_differ_from_neighbours() {
    for bank in BankId::shipped() {
        let raws: Vec<Vec<f32>> = (0..bank.variants())
            .map(|variant| export::raw(bank, variant))
            .collect();
        for (a, first) in raws.iter().enumerate() {
            for (b, second) in raws.iter().enumerate().skip(a + 1) {
                assert_eq!(first.len(), second.len(), "{bank:?}");
                // Two variants are different sounds, not one sound re-levelled.
                let energy: f32 = first.iter().map(|s| s * s).sum();
                let difference: f32 = first.iter().zip(second).map(|(x, y)| (x - y).powi(2)).sum();
                assert!(
                    difference > energy * 0.05,
                    "{bank:?} variants {a} and {b} are nearly the same clip"
                );
            }
        }
    }
    // No two files share a seed.
    let mut seeds: Vec<u64> = library().iter().map(|clip| clip.seed).collect();
    seeds.sort_unstable();
    seeds.dedup();
    assert_eq!(seeds.len(), library().len());
}

/// In-place radix-2 FFT of `(re, im)`.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -std::f32::consts::TAU / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (sin, cos) = (angle * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cos - im[b] * sin, re[b] * sin + im[b] * cos);
                (re[b], im[b]) = (re[a] - tr, im[a] - ti);
                (re[a], im[a]) = (re[a] + tr, im[a] + ti);
            }
        }
        len <<= 1;
    }
}

/// The longest time, in seconds, one frequency stands out as a note: its
/// spectral bin holds more than 12 dB (16x the power) over the mean of the
/// rest of its third-octave band, frame after frame.
///
/// Frames are 2048 samples (46 ms), 512 apart. Frames more than 30 dB under
/// the clip's loudest do not count: a decayed ring is not heard as a note.
/// Bins below 500 Hz do not count either: a sustained partial there is a thud
/// or a rumble, and its third-octave band is narrower than four bins.
fn longest_tonal_run(samples: &[f32]) -> f32 {
    const WINDOW: usize = 2048;
    const HOP: usize = 512;
    let bin_hz = RATE as f32 / WINDOW as f32;
    let first_bin = (500.0 / bin_hz).ceil() as usize;
    let last_bin = (8_000.0 / bin_hz) as usize;
    let mut padded = vec![0.0; WINDOW / 2];
    padded.extend_from_slice(samples);
    padded.resize(padded.len() + WINDOW, 0.0);
    let frames: Vec<Vec<f32>> = (0..(padded.len() - WINDOW) / HOP)
        .map(|frame| {
            let mut re: Vec<f32> = (0..WINDOW)
                .map(|i| {
                    let hann = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / WINDOW as f32).cos();
                    padded[frame * HOP + i] * hann
                })
                .collect();
            let mut im = vec![0.0; WINDOW];
            fft(&mut re, &mut im);
            (0..WINDOW / 2)
                .map(|k| re[k] * re[k] + im[k] * im[k])
                .collect()
        })
        .collect();
    let energy: Vec<f32> = frames.iter().map(|frame| frame.iter().sum()).collect();
    let loudest = energy.iter().copied().fold(0.0, f32::max);
    let mut run = vec![0_usize; last_bin + 2];
    let mut longest = 0;
    for (frame, power) in frames.iter().enumerate() {
        let audible = energy[frame] > loudest * 0.001;
        let mut tonal = vec![false; last_bin + 2];
        if audible {
            for k in first_bin..=last_bin {
                let low = (k as f32 / 2.0_f32.powf(1.0 / 6.0)).floor() as usize;
                let high = (k as f32 * 2.0_f32.powf(1.0 / 6.0)).ceil() as usize;
                let others: Vec<f32> = (low..=high)
                    .filter(|bin| bin.abs_diff(k) > 1)
                    .map(|bin| power[bin])
                    .collect();
                let mean = others.iter().sum::<f32>() / others.len() as f32;
                tonal[k] = power[k] > mean * 16.0;
            }
        }
        // A note may drift one bin between frames.
        let previous = run.clone();
        for k in first_bin..=last_bin {
            run[k] = if tonal[k] {
                1 + previous[k - 1].max(previous[k]).max(previous[k + 1])
            } else {
                0
            };
            longest = longest.max(run[k]);
        }
    }
    longest as f32 * HOP as f32 / RATE as f32
}

/// The interface sounds and the stingers (Victory plays Wave, Defeat plays
/// Structure, a withdrawal plays Retreat) carry no note: no music is shipped
/// (user decision), and a pitched jingle would read as one.
#[test]
fn ui_and_stinger_synthesis_is_non_melodic() {
    // The detector's own proof: a 0.3 s tone in noise is a note.
    let mut state = 1_u32;
    let tone: Vec<f32> = (0..RATE as usize / 2)
        .map(|i| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (state >> 8) as f32 / 8_388_608.0 - 1.0;
            let t = i as f32 / RATE as f32;
            let note = if t < 0.3 {
                (std::f32::consts::TAU * 880.0 * t).sin()
            } else {
                0.0
            };
            note * 0.5 + noise * 0.05
        })
        .collect();
    assert!(
        longest_tonal_run(&tone) > 0.25,
        "{}",
        longest_tonal_run(&tone)
    );

    let banks = [
        BankId::Ui(UiFamily::LootReveal),
        BankId::Ui(UiFamily::CoinBatch),
        Cue::Victory.bank(),
        Cue::Defeat.bank(),
        Cue::Retreat.bank(),
    ];
    for bank in banks {
        for clip in library().iter().filter(|clip| clip.bank == bank) {
            let run = longest_tonal_run(&clip.samples);
            assert!(run <= 0.15, "{} holds a note for {run:.3} s", clip.file);
        }
    }
}

/// `generator_sha256` is over LF-normalised sources, so a CRLF checkout of the
/// same commit records the same generator.
#[test]
fn survival_generator_sha256_matches_lf_normalised_sources() {
    let root = workspace_root();
    let input = export::read_generator_digest_input(&root).unwrap();
    let manifest = shipped_manifest();
    assert_eq!(
        manifest["generator_sha256"],
        hash(&input).as_str(),
        "a generator source changed: run the exporter with --write"
    );
    let listed: Vec<&str> = manifest["generator_sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source.as_str().unwrap())
        .collect();
    assert_eq!(listed, export::GENERATOR_SOURCES);
    let mut sorted = export::GENERATOR_SOURCES;
    sorted.sort_unstable();
    assert_eq!(sorted, export::GENERATOR_SOURCES, "sources are sorted");
    // Line ends do not change the digest; anything else does.
    let lf = [("a.rs", "one\ntwo\n".to_owned())];
    let crlf = [("a.rs", "one\r\ntwo\r\n".to_owned())];
    let other = [("a.rs", "one\ntwo \n".to_owned())];
    assert_eq!(
        export::generator_digest_input(&lf),
        export::generator_digest_input(&crlf)
    );
    assert_ne!(
        export::generator_digest_input(&lf),
        export::generator_digest_input(&other)
    );
    assert!(!String::from_utf8(input).unwrap().contains('\r'));
}

#[test]
fn schema2_survival_records_have_seed_and_true_peak() {
    let manifest = shipped_manifest();
    assert_eq!(manifest["schema"], 2);
    for key in ["recordings", "third_party", "ai_audio", "music"] {
        assert_eq!(manifest["policy"][key], false, "policy.{key}");
    }
    let records = manifest["files"].as_array().unwrap();
    assert_eq!(records.len(), 275);
    let banks = BankId::shipped();
    for record in records {
        let file = record["file"].as_str().unwrap();
        let bank = banks
            .iter()
            .find(|bank| bank.name() == record["bank"].as_str().unwrap())
            .unwrap_or_else(|| panic!("{file}: unknown bank"));
        let variant = record["variant"].as_u64().unwrap();
        assert_eq!(format!("audio/{file}"), bank.variant_path(variant));
        assert_eq!(
            record["seed"].as_u64(),
            Some(seed(*bank, variant)),
            "{file}"
        );
        assert_eq!(record["generator"], export::GENERATOR);
        assert_eq!(record["generator_sha256"], manifest["generator_sha256"]);
        assert_eq!(record["sample_rate"], 44_100);
        assert_eq!(record["channels"], 1);
        assert!(record["frames"].as_u64().unwrap() > 0);
        assert_eq!(record["sha256"].as_str().unwrap().len(), 64);
        let peak = record["peak"].as_f64().unwrap();
        let true_peak = record["true_peak"].as_f64().unwrap();
        assert!(peak > 0.0 && peak <= true_peak + 1e-6, "{file}");
        assert!(
            true_peak <= f64::from(CEILING),
            "{file}: true peak {true_peak}"
        );
        let loudness = record["mmax_lufs"].as_f64().unwrap();
        let target = record["target_mmax_lufs"].as_f64().unwrap();
        assert!(target <= TARGET_MMAX_LUFS, "{file}");
        assert!((loudness - target).abs() <= TOLERANCE_LU, "{file}");
        let reduction = record["limiter_gr_db"].as_f64().unwrap();
        assert!((0.0..=f64::from(MAX_GAIN_REDUCTION_DB)).contains(&reduction));
        assert!(record["seam_limit"].is_null());
    }
    // The v1 manifest it replaced is kept beside the other archived records.
    let v1: Value = serde_json::from_str(
        &std::fs::read_to_string(workspace_root().join(export::V1_MANIFEST)).unwrap(),
    )
    .unwrap();
    assert!(v1.get("schema").is_none());
    assert_eq!(v1["peak_limit"], 0.18);
    assert_eq!(manifest["v1"], export::V1_MANIFEST);
}

/// The files and the manifest on disk are exactly what the exporter renders,
/// and rendering again gives the same bytes.
#[test]
fn export_check_is_idempotent() {
    let root = workspace_root();
    let problems = export::check(&root, library(), &hash);
    assert!(problems.is_empty(), "{problems:#?}");
    let first = &library()[0];
    assert_eq!(&export::bank_clips(first.bank)[0], first);
    let generator = hash(&export::read_generator_digest_input(&root).unwrap());
    assert_eq!(
        export::manifest(&library()[..3], &generator, &hash),
        export::manifest(&library()[..3], &generator, &hash)
    );
}
