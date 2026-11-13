//! The v1 survival library is reproducible from `horde_audio::synth::legacy`
//! (A6-01): the synthesis moved out of `horde_game` without changing a sample.

use horde_audio::{
    bank::{LEGACY_FAMILIES, variant_path},
    cue::Cue,
    synth::legacy::{PEAK, RATE, VARIANTS, shipped, synthesize},
    wav,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `file name -> sha256` of one v1 survival manifest.
fn manifest_hashes(path: &Path) -> BTreeMap<String, String> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    json["files"]
        .as_array()
        .unwrap_or_else(|| panic!("{}: no files array", path.display()))
        .iter()
        .map(|record| {
            (
                record["file"].as_str().unwrap().to_owned(),
                record["sha256"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// The sha256 of every v1 file as exported on 2026-11-02: the 69 shipped
/// records plus the 3 archived NpcShot records. Once the wave-6 exporter
/// (A6-05) rewrites the shipped manifest as schema 2, the v1 records are its
/// verbatim copy `archive/audio/SURVIVAL_PROVENANCE.v1.json` (ruling S11).
fn v1_library() -> BTreeMap<String, String> {
    let root = workspace_root();
    let archived_v1 = root.join("archive/audio/SURVIVAL_PROVENANCE.v1.json");
    let shipped = if archived_v1.is_file() {
        archived_v1
    } else {
        root.join("assets/audio/SURVIVAL_PROVENANCE.json")
    };
    let mut library = manifest_hashes(&shipped);
    library.extend(manifest_hashes(
        &root.join("archive/audio/SURVIVAL_PROVENANCE.json"),
    ));
    library
}

#[test]
fn raw_synthesis_matches_v1_library() {
    let library = v1_library();
    assert_eq!(library.len(), 72, "24 families x 3 variants");
    let mut checked = 0;
    for family in LEGACY_FAMILIES {
        for variant in 0..VARIANTS {
            let path = variant_path(family, variant);
            let file = path.strip_prefix("audio/").unwrap();
            let recorded = library
                .get(file)
                .unwrap_or_else(|| panic!("{file} has no v1 record"));
            let bytes = wav::pcm16_bytes(RATE, &synthesize(family, variant));
            assert_eq!(
                &horde_core::sha256::hex(&bytes),
                recorded,
                "{file}: raw synthesis no longer matches the v1 library"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, library.len());
}

#[test]
fn shipped_v1_files_are_the_raw_synthesis() {
    // The manifest could drift from the files; this reads the files.
    let root = workspace_root();
    let v1_still_ships = !root
        .join("archive/audio/SURVIVAL_PROVENANCE.v1.json")
        .is_file();
    for family in LEGACY_FAMILIES {
        for variant in 0..VARIANTS {
            let path = variant_path(family, variant);
            let on_disk = if family.retired() {
                root.join("archive").join(&path)
            } else if v1_still_ships {
                root.join("assets").join(&path)
            } else {
                continue;
            };
            let bytes = std::fs::read(&on_disk)
                .unwrap_or_else(|error| panic!("{}: {error}", on_disk.display()));
            assert!(
                bytes == wav::pcm16_bytes(RATE, &synthesize(family, variant)),
                "{path} differs from the raw synthesis"
            );
        }
    }
}

#[test]
fn every_variant_is_repeatable_click_free_and_within_peak() {
    for cue in shipped() {
        for variant in 0..VARIANTS {
            let samples = synthesize(cue, variant);
            assert_eq!(samples, synthesize(cue, variant));
            assert!(
                samples
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= PEAK + 0.000001)
            );
            assert_eq!(samples[0], 0.0);
            assert_eq!(*samples.last().unwrap(), 0.0);
            let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
            assert!(
                (0.003..0.10).contains(&rms),
                "{cue:?} variant {variant} rms {rms}"
            );
            let mean = samples.iter().sum::<f32>() / samples.len() as f32;
            assert!(mean.abs() < 0.001, "{cue:?} mean {mean}");
        }
        assert_ne!(synthesize(cue, 0), synthesize(cue, 1));
    }
}

#[test]
fn weapon_reports_have_distinct_duration_and_energy_profiles() {
    let pistol = synthesize(Cue::Pistol, 0);
    let smg = synthesize(Cue::Smg, 0);
    let shotgun = synthesize(Cue::Shotgun, 0);
    let marksman = synthesize(Cue::Marksman, 0);
    assert!(
        smg.len() < pistol.len() && pistol.len() < shotgun.len() && shotgun.len() < marksman.len()
    );
    let late_energy = |samples: &[f32]| {
        samples
            .iter()
            .skip((RATE as f32 * 0.12) as usize)
            .map(|s| s * s)
            .sum::<f32>()
    };
    assert!(late_energy(&shotgun) > late_energy(&smg) * 2.0);
}

#[test]
fn pcm16_header_describes_mono_44100() {
    let bytes = wav::pcm16_bytes(RATE, &[0.0, 1.0, -1.0, 2.0]);
    assert_eq!(bytes.len(), 44 + 8);
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 36 + 8);
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
    assert_eq!(
        u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        44_100
    );
    assert_eq!(&bytes[36..40], b"data");
    let samples: Vec<i16> = bytes[44..]
        .chunks(2)
        .map(|pair| i16::from_le_bytes(pair.try_into().unwrap()))
        .collect();
    // Out-of-range samples clamp instead of wrapping.
    assert_eq!(samples, vec![0, 32767, -32767, 32767]);
}

#[test]
#[ignore = "Offline asset authoring: explicitly set LASTLIGHT_AUDIO_EXPORT to workspace root"]
fn export_survival_audio_assets_and_audition() {
    let root = PathBuf::from(
        std::env::var_os("LASTLIGHT_AUDIO_EXPORT").expect("Set LASTLIGHT_AUDIO_EXPORT"),
    );
    let assets = root.join("assets/audio");
    let audition = root.join("docs/audio");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::create_dir_all(&audition).unwrap();
    let mut reel = Vec::new();
    let mut index = String::from("cue,start_seconds,duration_seconds\n");
    // Retired families are archived; the exporter never ships them again.
    for cue in shipped() {
        for variant in 0..VARIANTS {
            let samples = synthesize(cue, variant);
            let path = root.join("assets").join(variant_path(cue, variant));
            wav::write_pcm16(&path, RATE, &samples).unwrap();
            if variant == 0 {
                index.push_str(&format!(
                    "{:?},{:.3},{:.3}\n",
                    cue,
                    reel.len() as f32 / RATE as f32,
                    samples.len() as f32 / RATE as f32
                ));
                reel.extend(samples);
                reel.resize(reel.len() + RATE as usize / 3, 0.0);
            }
        }
    }
    wav::write_pcm16(&audition.join("survival-audition.wav"), RATE, &reel).unwrap();
    std::fs::write(audition.join("survival-audition.csv"), index).unwrap();
}
