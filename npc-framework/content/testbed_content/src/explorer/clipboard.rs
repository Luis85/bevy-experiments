//! The OS clipboard through the platform's own tools (std only, no Bevy, no
//! Bevy `system_clipboard` feature: its `clipboard-win` dependency is
//! BSL-1.0, which `deny.toml` does not allow).
//!
//! [`copy`] pipes text into `clip.exe` / `pbcopy` / `wl-copy` / `xclip` /
//! `xsel`; [`paste`] reads it with PowerShell `Get-Clipboard` / `pbpaste` /
//! `wl-paste` / `xclip` / `xsel`. Both block the calling thread (callers run
//! them off the frame) for at most [`TOOL_TIMEOUT`] per tool; a tool that
//! hangs is killed and reported as timed out.

use std::io::{Read as _, Write as _};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

/// The longest one tool invocation is waited on.
pub const TOOL_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the waiter thread sleeps between `try_wait` calls.
const POLL: Duration = Duration::from_millis(10);

/// Puts `text` on the OS clipboard. `clip.exe` on Windows (fed UTF-16LE with
/// a BOM, it mangles non-ASCII UTF-8), `pbcopy` on macOS, else the first of
/// `wl-copy`, `xclip -selection clipboard`, `xsel --clipboard --input` that
/// works.
///
/// # Errors
/// When no tool starts and exits 0 in time (every tool's error, joined).
pub fn copy(text: &str) -> Result<(), String> {
    let utf8 = text.as_bytes().to_vec();
    let tools: Vec<(&str, &[&str], Vec<u8>)> = if cfg!(windows) {
        vec![("clip.exe", &[], utf16le_with_bom(text))]
    } else if cfg!(target_os = "macos") {
        vec![("pbcopy", &[], utf8)]
    } else {
        vec![
            ("wl-copy", &[], utf8.clone()),
            ("xclip", &["-selection", "clipboard"], utf8.clone()),
            ("xsel", &["--clipboard", "--input"], utf8),
        ]
    };
    let mut errors = Vec::new();
    for (tool, args, input) in tools {
        match run(tool, args, Some(input), TOOL_TIMEOUT) {
            Ok(_) => return Ok(()),
            Err(e) => errors.push(e),
        }
    }
    Err(errors.join("; "))
}

/// Reads the OS clipboard as text: `powershell -NoProfile -Command
/// Get-Clipboard` on Windows (its output switched to UTF-8 first, else the
/// console code page mangles non-ASCII), `pbpaste` on macOS, else the first
/// of `wl-paste --no-newline --type text/plain` (non-text content is not
/// pasted), `xclip -selection clipboard -o`, `xsel --clipboard --output`
/// that works.
///
/// # Errors
/// When no tool starts and exits 0 in time (every tool's error, joined).
pub fn paste() -> Result<String, String> {
    let tools: &[(&str, &[&str])] = if cfg!(windows) {
        &[(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Get-Clipboard",
            ],
        )]
    } else if cfg!(target_os = "macos") {
        &[("pbpaste", &[])]
    } else {
        &[
            ("wl-paste", &["--no-newline", "--type", "text/plain"]),
            ("xclip", &["-selection", "clipboard", "-o"]),
            ("xsel", &["--clipboard", "--output"]),
        ]
    };
    let mut errors = Vec::new();
    for (tool, args) in tools {
        match run(tool, args, None, TOOL_TIMEOUT) {
            Ok(out) => return Ok(decode_paste(&out)),
            Err(e) => errors.push(e),
        }
    }
    Err(errors.join("; "))
}

/// The text a paste into a field inserts: for a single-line field the first
/// non-blank line (without its CR / LF), else every line with CR LF made LF
/// and the trailing line break dropped (`Get-Clipboard` ends every line with
/// CR LF). Shared by the menu's fields and the F7 notes editor.
#[must_use]
pub fn paste_text(raw: &str, single_line: bool) -> String {
    if single_line {
        return raw
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or_default()
            .to_owned();
    }
    raw.lines().collect::<Vec<_>>().join("\n")
}

/// `text` as UTF-16LE bytes after a byte-order mark (what `clip.exe` reads).
#[must_use]
pub fn utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// A tool's stdout as text: UTF-8 (lossy) without the byte-order mark some
/// PowerShell builds print first.
fn decode_paste(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout)
        .trim_start_matches('\u{feff}')
        .to_owned()
}

/// How long after the tool exited its helper threads are still waited on.
const GRACE: Duration = Duration::from_secs(1);

/// Runs `tool args`. With `input` (a copy) the text is written to its stdin
/// and stdout is discarded: wl-copy / xclip leave a background selection
/// owner holding any inherited pipe, which must never be waited on. Without
/// `input` (a paste) stdout is collected. The tool gets at most `timeout`
/// (true time, through `recv_timeout`: no clock read) and is killed and
/// reaped after that; a thread that might block is never joined.
fn run(
    tool: &str,
    args: &[&str],
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let mut cmd = Command::new(tool);
    cmd.args(args).stderr(Stdio::null());
    if input.is_some() {
        cmd.stdin(Stdio::piped()).stdout(Stdio::null());
    } else {
        cmd.stdin(Stdio::null()).stdout(Stdio::piped());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // No console window flashes up for the tool.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("{tool}: {e}"))?;
    let (write_tx, write_rx) = mpsc::channel();
    if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
        // Dropping stdin closes the pipe, so the tool sees the end of the text.
        std::thread::spawn(move || {
            let _ = write_tx.send(stdin.write_all(&input));
        });
    }
    let (read_tx, read_rx) = mpsc::channel();
    let capture = child.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = read_tx.send(stdout.read_to_end(&mut buf).map(|_| buf));
        });
    });
    // The waiter owns the child: it reports the exit, or kills and reaps the
    // child once `kill` is set.
    let kill = Arc::new(AtomicBool::new(false));
    let (exit_tx, exit_rx) = mpsc::channel();
    let flag = Arc::clone(&kill);
    std::thread::spawn(move || {
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if flag.load(Ordering::Relaxed) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                Ok(None) => std::thread::sleep(POLL),
                Err(e) => break Err(e),
            }
        };
        let _ = exit_tx.send(status);
    });
    let status = match exit_rx.recv_timeout(timeout) {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => return Err(format!("{tool}: {e}")),
        Err(_) => {
            kill.store(true, Ordering::Relaxed);
            return Err(format!(
                "{tool}: timed out after {} ms",
                timeout.as_millis()
            ));
        }
    };
    if !status.success() {
        return Err(format!("{tool} exited with {status}"));
    }
    // A write that has not finished once the tool exited is a closed pipe
    // about to fail: not worth waiting for.
    if let Ok(Err(e)) = write_rx.recv_timeout(GRACE) {
        return Err(format!("{tool}: writing the text: {e}"));
    }
    if capture.is_none() {
        return Ok(Vec::new());
    }
    match read_rx.recv_timeout(GRACE) {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(format!("{tool}: reading the text: {e}")),
        Err(_) => Err(format!("{tool}: its output never closed")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16le_with_bom_encodes_clip_exe_input() {
        assert_eq!(utf16le_with_bom(""), [0xFF, 0xFE]);
        assert_eq!(
            utf16le_with_bom("A\u{e9}\u{20ac}"),
            [0xFF, 0xFE, 0x41, 0x00, 0xE9, 0x00, 0xAC, 0x20]
        );
        // A surrogate pair (U+1F600 = D83D DE00).
        assert_eq!(
            utf16le_with_bom("\u{1F600}"),
            [0xFF, 0xFE, 0x3D, 0xD8, 0x00, 0xDE]
        );
    }

    #[test]
    fn paste_output_loses_a_byte_order_mark_only() {
        assert_eq!(decode_paste(b"\xEF\xBB\xBFhi\r\n"), "hi\r\n");
        assert_eq!(decode_paste(b"plain"), "plain");
        assert_eq!(decode_paste("\u{e9}".as_bytes()), "\u{e9}");
    }

    #[test]
    fn a_missing_tool_is_an_error() {
        let e = run("npc-no-such-copy-tool", &[], None, TOOL_TIMEOUT).unwrap_err();
        assert!(e.starts_with("npc-no-such-copy-tool:"), "{e}");
    }

    #[test]
    fn a_hung_tool_is_killed_after_the_timeout() {
        let (tool, args): (&str, &[&str]) = if cfg!(windows) {
            ("ping", &["-n", "30", "127.0.0.1"])
        } else {
            ("sleep", &["30"])
        };
        let e = run(tool, args, None, Duration::from_millis(300)).unwrap_err();
        assert!(e.contains("timed out"), "{e}");
    }

    /// A tool that exits at once but leaves a grandchild running (as wl-copy
    /// and xclip leave their selection owner): a copy must not wait for it,
    /// because stdout is not piped to a copy.
    #[test]
    fn a_copy_does_not_wait_for_a_lingering_grandchild() {
        let (tool, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/c", "start /b ping -n 6 127.0.0.1 >nul"])
        } else {
            ("sh", &["-c", "sleep 5 &"])
        };
        run(tool, args, Some(b"x".to_vec()), TOOL_TIMEOUT).unwrap();
    }

    /// A paste whose output pipe a grandchild keeps open is an error after the
    /// grace period, not a hang.
    #[cfg(unix)]
    #[test]
    fn a_paste_with_a_lingering_grandchild_is_bounded() {
        let e = run("sh", &["-c", "sleep 5 &"], None, TOOL_TIMEOUT).unwrap_err();
        assert!(e.contains("never closed"), "{e}");
    }

    /// Run by hand: `cargo test -p testbed_content os_clipboard -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "overwrites the OS clipboard"]
    fn os_clipboard_round_trip() {
        let text = "npc explorer \u{e9}\u{20ac} line 1\nline 2";
        copy(text).unwrap();
        let back = paste().unwrap();
        assert_eq!(back.trim_end().replace("\r\n", "\n"), text);
    }
}
