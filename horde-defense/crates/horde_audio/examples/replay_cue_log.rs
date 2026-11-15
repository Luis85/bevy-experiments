//! Replays a review cue log and meters it (A6-18).
//!
//! ```text
//! cargo run --locked -p horde_audio --example replay_cue_log -- <cues.jsonl>
//!     [--assets <dir>] [--wav <out.wav>]
//! ```
//!
//! `<cues.jsonl>` is the log a review session writes with
//! `LASTLIGHT_REVIEW_CUE_LOG` (by convention `target/review/<run>/cues.jsonl`).
//! The replay renders it with the shipped clips and prints its BS.1770-4
//! loudness beside the loudness gate's target, so a real session can be held
//! against `reference_mixed_60s` (-23 +/-2 LUFS integrated, true peak <= 0.95).
//! `--wav` also writes the left and right signals as `<out>.L.wav` and
//! `<out>.R.wav` for listening.
#![allow(clippy::print_stdout, clippy::print_stderr)] // a CLI: its report is its output

use horde_audio::{cue_log, render, wav};
use std::path::{Path, PathBuf};

fn run() -> Result<(), String> {
    let mut log = None;
    let mut assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut out: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--assets" => assets = args.next().ok_or("--assets needs a folder")?.into(),
            "--wav" => out = Some(args.next().ok_or("--wav needs a path")?.into()),
            _ if log.is_none() && !arg.starts_with("--") => log = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument {arg}")),
        }
    }
    let log = log.ok_or("usage: replay_cue_log <cues.jsonl> [--assets <dir>] [--wav <out.wav>]")?;
    let text =
        std::fs::read_to_string(&log).map_err(|error| format!("{}: {error}", log.display()))?;
    let frames = cue_log::parse(&text).map_err(|error| format!("{}: {error}", log.display()))?;
    let library = render::Library::load(&assets)?;
    let signal = cue_log::replay(&library, &frames)?;
    let starts: usize = frames.iter().map(|frame| frame.starts.len()).sum();
    let fades: usize = frames.iter().map(|frame| frame.fades.len()).sum();
    let seconds = signal.left.len() as f64 / f64::from(render::RATE);
    println!(
        "{}: {} frames, {seconds:.1} s, {starts} starts, {fades} fades",
        log.display(),
        frames.len()
    );
    if signal.left.is_empty() {
        return Err("the log is empty".into());
    }
    let loudness = signal.loudness();
    println!(
        "I {:.1} LUFS, S-max {:.1}, M-max {:.1}, sample peak {:.3}, true peak {:.3}",
        loudness.integrated,
        loudness.short_term_max,
        loudness.momentary_max,
        loudness.sample_peak,
        loudness.true_peak
    );
    println!(
        "gate (reference_mixed_60s): -23 +/-2 LUFS integrated, true peak <= 0.95; delta to -23: {:+.1} LU",
        loudness.integrated + 23.0
    );
    if let Some(out) = out {
        for (side, samples) in [("L", &signal.left), ("R", &signal.right)] {
            let path = out.with_extension(format!("{side}.wav"));
            wav::write_pcm16(&path, render::RATE, samples)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            println!("wrote {}", path.display());
        }
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
