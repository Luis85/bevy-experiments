//! The survival audio exporter.
//!
//! ```text
//! cargo run --locked -p horde_audio --example export_audio              # check
//! cargo run --locked -p horde_audio --example export_audio -- --write   # write
//! ```
//!
//! `--check` (the default) renders the library in memory, compares it with
//! `assets/audio/survival_*.wav` and `SURVIVAL_PROVENANCE.json`, prints one
//! line per difference, writes nothing and exits 1 on a difference.
//!
//! `--write` writes the files and the manifest. The first time, it copies the
//! v1 manifest verbatim to `archive/audio/SURVIVAL_PROVENANCE.v1.json`. It
//! also writes the audition reel to `target/audio-audition/`, which is never
//! committed. `--root <folder>` names the workspace folder (default: the one
//! this crate lives in).
#![allow(clippy::print_stdout, clippy::print_stderr)] // a CLI: its report is its output

use horde_audio::{export, synth::legacy::RATE, wav};
use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let mut write = false;
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--check" => write = false,
            "--write" => write = true,
            "--root" => match arguments.next() {
                Some(folder) => root = PathBuf::from(folder),
                None => {
                    eprintln!("--root needs a folder");
                    return ExitCode::from(2);
                }
            },
            other => {
                eprintln!("unknown argument {other}; use --check, --write or --root <folder>");
                return ExitCode::from(2);
            }
        }
    }
    let hash = |bytes: &[u8]| horde_core::sha256::hex(bytes);
    let clips = export::library();
    if write {
        if let Err(error) = write_library(&root, &clips, &hash) {
            eprintln!("export failed: {error}");
            return ExitCode::FAILURE;
        }
        println!("wrote {} files and {}", clips.len(), export::MANIFEST);
    }
    let problems = export::check(&root, &clips, &hash);
    for problem in &problems {
        println!("{problem}");
    }
    if problems.is_empty() {
        println!("{} survival files match the export", clips.len());
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn write_library(
    root: &std::path::Path,
    clips: &[export::Clip],
    hash: &dyn Fn(&[u8]) -> String,
) -> std::io::Result<()> {
    let audio = root.join("assets/audio");
    // The v1 manifest is archived once, byte for byte, before it is replaced.
    let v1 = root.join(export::V1_MANIFEST);
    if !v1.is_file() {
        let current = std::fs::read_to_string(root.join(export::MANIFEST))?;
        if current.contains("\"schema\": 2") {
            return Err(std::io::Error::other(format!(
                "{} is missing and the shipped manifest is already schema 2",
                export::V1_MANIFEST
            )));
        }
        std::fs::copy(root.join(export::MANIFEST), &v1)?;
    }
    for clip in clips {
        std::fs::write(audio.join(&clip.file), clip.bytes())?;
    }
    let generator = hash(&export::read_generator_digest_input(root)?);
    std::fs::write(
        root.join(export::MANIFEST),
        export::manifest(clips, &generator, hash),
    )?;
    let audition = root.join("target/audio-audition");
    std::fs::create_dir_all(&audition)?;
    let (reel, index) = export::audition_reel(clips);
    wav::write_pcm16(&audition.join("survival-audition.wav"), RATE, &reel)?;
    std::fs::write(audition.join("survival-audition.csv"), index)
}
