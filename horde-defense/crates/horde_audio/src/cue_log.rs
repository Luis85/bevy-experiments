//! The review cue log: what the executor did each frame, as JSON lines, and
//! the replay that renders a log back to the signal it describes.
//!
//! The game's review service writes one (`LASTLIGHT_REVIEW_CUE_LOG`, by
//! convention `target/review/<run>/cues.jsonl`) from the voices its `SfxPlugin`
//! really started, and [`crate::render::render_logged`] writes the same lines
//! for a scripted scene. `examples/replay_cue_log.rs` meters a log, so a real
//! session can be held against the loudness gate's reference scene.
//!
//! The file is a header line `{"cue_log":1}` and then one [`CueLogFrame`] per
//! line. A frame carries what an executor needs and nothing it has to decide:
//! the clip of each start by path, its speed, low-pass and final
//! `[left, right]` gains, the voices that fade, the six ambience gains the
//! loops reached and the density gain. Times are whole microseconds, so a log
//! reads back exactly.
//!
//! What a replay leaves out: a slider or effects-mode change on a voice that
//! already sounds (the log has each voice's gains at its start), and the
//! device's resampling.

use crate::render::{FrameActions, Library, Playing, RATE, Stereo, VoiceStart, joined, mix_loops};
use serde::{Deserialize, Serialize};

/// The log's format version, the header line's `cue_log`.
pub const VERSION: u32 = 1;

/// A cue's name in the log: its Rust identifier ("Pistol", "LootReveal"),
/// for whoever reads the log. Both writers (the offline render and the
/// game's recorder) name cues through this one function.
pub fn cue_name(cue: crate::cue::Cue) -> String {
    format!("{cue:?}")
}

/// One started voice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoggedStart {
    /// The executor's id of the voice; a later [`CueLogFrame::fades`] names it.
    pub voice: u64,
    /// The cue's name, for the reader. The replay does not use it.
    pub cue: String,
    /// The clip, relative to the asset folder.
    pub clip: String,
    /// Playback speed: the start's jitter times its voicing's.
    pub speed: f32,
    /// The voicing's low-pass cutoff, if it has one.
    pub lowpass_hz: Option<f32>,
    /// `[left, right]`: pan, bus, row, jitter and voicing gains multiplied.
    pub gains: [f32; 2],
    /// The voice follows the frames' density gain.
    pub compensated: bool,
}

/// One frame of the executor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CueLogFrame {
    /// When the frame began, in microseconds on the executor's clock.
    pub us: u64,
    /// The frame's length in microseconds. Only the last frame's is used:
    /// the others end where the next begins.
    pub dt_us: u64,
    /// The voices that start their fade this frame, before the starts.
    pub fades: Vec<u64>,
    pub starts: Vec<LoggedStart>,
    /// The gains the six ambience loops reach by the end of the frame, in
    /// [`crate::bank::AMBIENCE_FILES`] order, duck and calibration included.
    pub ambience: [f32; 6],
    /// The density gain the compensated voices ramp to.
    pub density: f32,
}
impl CueLogFrame {
    /// The frame an executor logs for `actions`: `gains` gives the
    /// `[left, right]` gains it plays a start at.
    pub fn from_actions(
        us: u64,
        dt_us: u64,
        actions: &FrameActions,
        gains: impl Fn(&VoiceStart) -> [f32; 2],
        ambience: [f32; 6],
        density: f32,
    ) -> Self {
        Self {
            us,
            dt_us,
            fades: actions.fades.clone(),
            starts: actions
                .starts
                .iter()
                .map(|start| LoggedStart {
                    voice: start.voice,
                    cue: cue_name(start.cue),
                    clip: start.voicing.bank.variant_path(start.variant),
                    speed: start.jitter.speed * start.voicing.speed,
                    lowpass_hz: start.voicing.lowpass_hz,
                    gains: gains(start),
                    compensated: start.compensated,
                })
                .collect(),
            ambience,
            density,
        }
    }

    /// The frame as one JSON line, without the line break.
    pub fn line(&self) -> String {
        serde_json::to_string(self).expect("a cue-log frame is plain data")
    }
}

/// The header line, without the line break.
pub fn header() -> String {
    format!("{{\"cue_log\":{VERSION}}}")
}

/// The whole log: the header and one line per frame.
pub fn write(frames: &[CueLogFrame]) -> String {
    let mut text = header();
    text.push('\n');
    for frame in frames {
        text.push_str(&frame.line());
        text.push('\n');
    }
    text
}

/// Reads a log. Blank lines are skipped; a missing header, another version or
/// a malformed line is an error that names its line.
pub fn parse(text: &str) -> Result<Vec<CueLogFrame>, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Header {
        cue_log: u32,
    }
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let (_, first) = lines.next().ok_or("the cue log is empty")?;
    let header: Header = serde_json::from_str(first)
        .map_err(|error| format!("line 1 is not a cue-log header: {error}"))?;
    if header.cue_log != VERSION {
        return Err(format!(
            "cue log version {} is not version {VERSION}",
            header.cue_log
        ));
    }
    lines
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", index + 1))
        })
        .collect()
}

/// Renders `frames` at [`RATE`]: the signal starts at the first frame and
/// ends where the last frame does. The executor is the offline render's
/// ([`crate::render::render_logged`]): fades first, then the starts, the
/// density gain, and the ambience loops ramped across the frame.
pub fn replay(library: &Library, frames: &[CueLogFrame]) -> Result<Stereo, String> {
    let Some((first, last)) = frames.first().zip(frames.last()) else {
        return Ok(Stereo {
            left: Vec::new(),
            right: Vec::new(),
        });
    };
    if let Some(pair) = frames.windows(2).find(|pair| pair[1].us < pair[0].us) {
        return Err(format!(
            "the frame at {} us follows the frame at {} us",
            pair[1].us, pair[0].us
        ));
    }
    // Microseconds to samples, rounded to the nearest.
    let sample_at = |us: u64| -> usize {
        ((us - first.us) as u128 * u128::from(RATE) + 500_000) as usize / 1_000_000
    };
    let total = sample_at(last.us + last.dt_us);
    let mut out = vec![0.0_f32; total];
    let mut sides = [out.clone(), out.clone()];
    let mut voices: Vec<(u64, Playing)> = Vec::new();
    let mut loop_gains = [0.0_f32; 6];
    for (index, frame) in frames.iter().enumerate() {
        let offset = sample_at(frame.us);
        let count = frames
            .get(index + 1)
            .map_or(total, |next| sample_at(next.us))
            - offset;
        voices.retain(|(_, playing)| !playing.ended());
        for (id, playing) in &voices {
            if frame.fades.contains(id) {
                playing.control.fade_out();
            }
        }
        for start in &frame.starts {
            let clip = library
                .path_clip(&start.clip)
                .ok_or_else(|| format!("the library has no clip {}", start.clip))?;
            voices.push((
                start.voice,
                Playing::start(
                    clip,
                    start.speed,
                    start.lowpass_hz,
                    start.gains,
                    start.compensated,
                ),
            ));
        }
        for (_, playing) in &mut voices {
            playing.follow_density(frame.density);
            playing.mix_into(&mut sides, offset, count);
        }
        mix_loops(
            library,
            &mut out[offset..offset + count],
            offset,
            loop_gains,
            frame.ambience,
        );
        loop_gains = frame.ambience;
    }
    Ok(joined(sides, &out))
}
