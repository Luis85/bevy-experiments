//! Private, append-only Markdown transcript of one client process lifetime.
#[path = "log_writer.rs"]
mod writer;
use super::{Provider, storage};
use npc_framework::agent_io::registration::new_secret;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use std::fs::File;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const IO_ERROR: &str = "Cannot persist AI client Markdown log; check --logs-dir and folder permissions before restarting";

#[derive(Clone)]
pub struct SessionLog(Arc<Inner>);
struct Inner {
    writer: writer::Writer,
    secrets: Vec<String>,
    history: Arc<Mutex<HistoryRecord>>,
    private: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryOwner {
    version: u32,
    owner_id: String,
}
#[derive(Clone, Serialize)]
struct HistoryRecord {
    version: u32,
    owner_id: String,
    kind: &'static str,
    id: String,
    path: PathBuf,
    provider: Provider,
    created_unix_seconds: u64,
    metrics_id: Option<String>,
}
fn history_owner(private: &Path) -> Result<String, String> {
    fs::create_dir_all(private).map_err(|_| IO_ERROR)?;
    let path = private.join("history-owner.json");
    if path.exists() {
        let owner: HistoryOwner = storage::read_json(&path).map_err(|_| IO_ERROR)?;
        if owner.version != 1 || !super::metrics::valid_id(&owner.owner_id) {
            return Err(IO_ERROR.into());
        }
        return Ok(owner.owner_id);
    }
    let owner = HistoryOwner {
        version: 1,
        owner_id: new_secret().map_err(|_| IO_ERROR)?[..32].to_owned(),
    };
    storage::write_json(&path, &owner).map_err(|_| IO_ERROR)?;
    Ok(owner.owner_id)
}
fn index_record(private: &Path, record: &HistoryRecord) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(record).map_err(|_| IO_ERROR)?;
    if bytes.len() > 16 * 1024 {
        return Err(IO_ERROR.into());
    }
    bytes.push(b'\n');
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(private.join("history-index.jsonl"))
        .and_then(|mut file| file.write_all(&bytes).and_then(|()| file.sync_data()))
        .map_err(|_| IO_ERROR.into())
}

impl SessionLog {
    pub fn open(
        private: &Path,
        explicit: Option<&Path>,
        provider: Provider,
        token: &str,
    ) -> Result<Self, String> {
        let directory = explicit
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("NPC_AI_LOGS_DIR").map(Into::into))
            .unwrap_or_else(|| private.join("logs"));
        if directory.as_os_str().is_empty() {
            return Err(IO_ERROR.into());
        }
        let directory = if directory.is_absolute() {
            directory
        } else {
            std::env::current_dir()
                .map_err(|_| IO_ERROR)?
                .join(directory)
        };
        let owner_id = history_owner(private)?;
        let id = new_secret().map_err(|_| IO_ERROR)?;
        let created_unix_seconds = storage::now();
        let id = format!("{created_unix_seconds}-{}", &id[..32]);
        let path = directory.join(format!("{id}.md"));
        let history = HistoryRecord {
            version: 1,
            owner_id,
            kind: "log",
            id,
            path,
            provider,
            created_unix_seconds,
            metrics_id: None,
        };
        let history = Arc::new(Mutex::new(history));
        let worker_history = history.clone();
        let worker_private = private.to_path_buf();
        let writer = writer::Writer::spawn(move || {
            fs::create_dir_all(&directory).map_err(|_| IO_ERROR)?;
            let directory = directory.canonicalize().map_err(|_| IO_ERROR)?;
            let mut record = worker_history.lock().map_err(|_| IO_ERROR)?.clone();
            record.path = directory.join(format!("{}.md", record.id));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&record.path).map_err(|_| IO_ERROR)?;
            file.write_all(b"# AI agent game client session\n\nPrivate gameplay transcript. Credentials and session identifiers are redacted. Only output emitted by the owned CLI is available; no provider history is read. A missing final entry indicates an interrupted client.\n")
                .and_then(|()| file.sync_data()).map_err(|_| IO_ERROR)?;
            index_record(&worker_private, &record)?;
            *worker_history.lock().map_err(|_| IO_ERROR)? = record;
            Ok(Box::new(file))
        })?;
        let log = Self(Arc::new(Inner {
            writer,
            secrets: vec![token.to_owned()],
            history,
            private: private.to_path_buf(),
        }));
        log.entry(
            "Client started",
            &serde_json::json!({"provider":provider}).to_string(),
        )?;
        Ok(log)
    }

    /// Register exact owned exports; native readers never scan shared output folders.
    pub fn link_metrics(&self, directory: &Path, metrics_id: &str) -> Result<(), String> {
        if !super::metrics::valid_id(metrics_id) {
            return Err(IO_ERROR.into());
        }
        let path = directory
            .join(format!("{metrics_id}.json"))
            .canonicalize()
            .map_err(|_| IO_ERROR)?;
        // Setup-only durable barrier; no provider turn has been dispatched yet.
        self.flush(Duration::from_secs(2))?;
        let mut history = self.0.history.lock().map_err(|_| IO_ERROR)?.clone();
        let metrics = HistoryRecord {
            kind: "metrics",
            id: metrics_id.to_owned(),
            path,
            metrics_id: None,
            ..history.clone()
        };
        index_record(&self.0.private, &metrics)?;
        history.metrics_id = Some(metrics_id.to_owned());
        index_record(&self.0.private, &history)?;
        *self.0.history.lock().map_err(|_| IO_ERROR)? = history;
        Ok(())
    }

    pub fn check(&self) -> Result<(), String> {
        self.0.writer.check()
    }

    /// Bounded durable barrier before provider dispatch, never an unbounded I/O lock.
    pub fn flush(&self, timeout: Duration) -> Result<(), String> {
        self.0.writer.flush(timeout)
    }
    /// Finish accepted output and close the file within the caller's total deadline.
    pub fn shutdown(&self, timeout: Duration) -> Result<(), String> {
        self.0.writer.shutdown(timeout)
    }

    #[cfg(test)]
    pub(super) fn fail_writes_for_test(&self, read_only_file: &Path) {
        self.0
            .writer
            .replace_file(File::open(read_only_file).unwrap())
            .unwrap();
        self.entry("Failure fixture", "owned write failure")
            .unwrap();
        assert!(self.flush(Duration::from_secs(2)).is_err());
    }

    pub fn entry(&self, title: &str, text: &str) -> Result<(), String> {
        self.check()?;
        if text.len() > 512 * 1024 {
            return self.0.writer.reject_capacity();
        }
        let clean = sanitize(text, &self.0.secrets);
        self.clean_entry(title, &clean)
    }
    fn clean_entry(&self, title: &str, clean: &str) -> Result<(), String> {
        let mut entry = format!("\n## {title} — Unix {}\n\n", storage::now());
        let encoded = clean.lines().try_fold(entry.len(), |total, line| {
            total.checked_add(line.len())?.checked_add(5)
        });
        if encoded.is_none_or(|size| size > writer::MAX_QUEUED_BYTES) {
            return self.0.writer.reject_capacity();
        }
        // Indented blocks keep arbitrary Markdown/HTML/fences from becoming document structure.
        for line in clean.lines() {
            entry.push_str("    ");
            entry.push_str(line);
            entry.push('\n');
        }
        self.0.writer.entry(entry.into_bytes())
    }

    pub fn reader<R: Read>(&self, reader: R, stream: &'static str, limit: usize) -> LogReader<R> {
        LogReader {
            reader,
            log: self.clone(),
            stream,
            limit,
            total: 0,
            line: Vec::new(),
            oversized: false,
            capped: false,
            batch: String::new(),
            encoded: 0,
            encoded_limit: limit.saturating_mul(2).saturating_add(64 * 1024),
        }
    }
}

fn sensitive_key(key: &str) -> bool {
    let key: String = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    key == "token"
        || key.ends_with("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("credential")
        || key.contains("authorization")
        || key.contains("apikey")
        || matches!(
            key.as_str(),
            "session"
                | "sessionid"
                | "sessionkey"
                | "threadid"
                | "owner"
                | "encryptedcontent"
                | "signature"
        )
}

fn sanitize_value(value: &mut Value, secrets: &[String], depth: usize) {
    if depth > 32 {
        *value = "[redacted: nesting limit]".into();
        return;
    }
    match value {
        Value::Object(object) => {
            let original = std::mem::take(object);
            let reserved: std::collections::BTreeSet<_> = original.keys().cloned().collect();
            for (index, (key, mut value)) in original.into_iter().enumerate() {
                if sensitive_key(&key) {
                    value = "[redacted]".into();
                } else {
                    sanitize_value(&mut value, secrets, depth + 1);
                }
                let clean_key = sanitize_text(&key, secrets);
                let clean_key = if clean_key == key {
                    key
                } else {
                    let mut replacement = format!("[redacted key {index}]");
                    while reserved.contains(&replacement) || object.contains_key(&replacement) {
                        replacement.push('_');
                    }
                    replacement
                };
                object.insert(clean_key, value);
            }
        }
        Value::Array(array) => {
            for value in array {
                sanitize_value(value, secrets, depth + 1);
            }
        }
        Value::String(text) => {
            // MCP content often embeds JSON as a string, including registration credentials.
            if let Ok(mut nested) = serde_json::from_str::<Value>(text) {
                sanitize_value(&mut nested, secrets, depth + 1);
                *text = nested.to_string();
            } else {
                *text = sanitize_text(text, secrets);
            }
        }
        _ => {}
    }
}

fn sanitize(text: &str, secrets: &[String]) -> String {
    if let Ok(mut value) = serde_json::from_str::<Value>(text) {
        sanitize_value(&mut value, secrets, 0);
        // Compact JSON avoids indentation amplification before the encoded budget.
        serde_json::to_string(&value).unwrap_or_else(|_| "[unavailable]".into())
    } else {
        sanitize_text(text, secrets)
    }
}

fn sanitize_text(text: &str, secrets: &[String]) -> String {
    let mut text = text.to_owned();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        text = text.replace(secret, "[redacted]");
    }
    // Generated game tokens/keys use 64 hex digits; redact UUIDs wherever echoed too.
    let mut clean = String::new();
    let mut rest = text.as_str();
    while !rest.is_empty() {
        let hex = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
        if hex >= 64 {
            clean.push_str("[redacted]");
            rest = &rest[hex..];
        } else if rest.len() >= 36
            && rest.is_char_boundary(36)
            && super::provider::uuid(&rest[..36])
        {
            clean.push_str("[redacted]");
            rest = &rest[36..];
        } else {
            let ch = rest.chars().next().expect("nonempty");
            // Omit terminal control sequences; preserve newlines and tabs for readability.
            if !ch.is_control() || matches!(ch, '\n' | '\t') {
                clean.push(ch);
            }
            rest = &rest[ch.len_utf8()..];
        }
    }
    scrub_assignments(&clean)
}

fn value_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len()
        && !bytes[end].is_ascii_whitespace()
        && !matches!(
            bytes[end],
            b'&' | b';' | b',' | b')' | b']' | b'}' | b'"' | b'\''
        )
    {
        end += 1;
    }
    end
}
fn scrub_assignments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut clean = String::with_capacity(text.len());
    let (mut cursor, mut copied) = (0, 0);
    while cursor < bytes.len() {
        if !bytes[cursor].is_ascii_alphabetic() && bytes[cursor] != b'-' {
            cursor += text[cursor..].chars().next().expect("nonempty").len_utf8();
            continue;
        }
        let start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric() || matches!(bytes[cursor], b'_' | b'-'))
        {
            cursor += 1;
        }
        let key = &text[start..cursor];
        let bearer = key.eq_ignore_ascii_case("bearer");
        if !sensitive_key(key) && !bearer {
            continue;
        }
        let mut separator = cursor;
        if separator < bytes.len() && matches!(bytes[separator], b'"' | b'\'') {
            separator += 1;
        }
        while separator < bytes.len() && matches!(bytes[separator], b' ' | b'\t') {
            separator += 1;
        }
        if key.eq_ignore_ascii_case("session")
            && bytes
                .get(separator..separator + 3)
                .is_some_and(|part| part.eq_ignore_ascii_case(b"key"))
        {
            separator += 3;
            while separator < bytes.len() && matches!(bytes[separator], b' ' | b'\t') {
                separator += 1;
            }
        }
        let assigned = separator < bytes.len() && matches!(bytes[separator], b'=' | b':');
        let space_value = (bearer || key.starts_with("--")) && separator > cursor;
        if !assigned && !space_value {
            continue;
        }
        let mut value = separator + usize::from(assigned);
        while value < bytes.len() && matches!(bytes[value], b' ' | b'\t') {
            value += 1;
        }
        if value == bytes.len() {
            break;
        }
        let mut end;
        if matches!(bytes[value], b'"' | b'\'') {
            let quote = bytes[value];
            value += 1;
            end = value;
            while end < bytes.len() && bytes[end] != quote {
                end += if bytes[end] == b'\\' && end + 1 < bytes.len() {
                    2
                } else {
                    1
                };
            }
        } else {
            end = value_end(bytes, value);
            if key.eq_ignore_ascii_case("authorization")
                && matches!(
                    text[value..end].to_ascii_lowercase().as_str(),
                    "bearer" | "basic"
                )
            {
                while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
                    end += 1;
                }
                end = value_end(bytes, end);
            }
        }
        if end > value {
            clean.push_str(&text[copied..value]);
            clean.push_str("[redacted]");
            copied = end;
            cursor = end;
        }
    }
    clean.push_str(&text[copied..]);
    clean
}

/// Tee bounded complete lines without changing the provider parser's bytes or backpressure.
pub struct LogReader<R> {
    reader: R,
    log: SessionLog,
    stream: &'static str,
    limit: usize,
    total: usize,
    line: Vec<u8>,
    oversized: bool,
    capped: bool,
    batch: String,
    encoded: usize,
    encoded_limit: usize,
}
impl<R> LogReader<R> {
    fn flush_batch(&mut self) {
        if !self.batch.is_empty() {
            let clean = std::mem::take(&mut self.batch);
            // Reserve an upper bound for the fixed header within this stream's
            // encoded budget. Queue pressure itself latches a visible failure.
            self.encoded = self.encoded.saturating_add(128);
            let _ = self.log.clean_entry(self.stream, &clean);
        }
    }
    fn cap(&mut self, message: &str) {
        if !self.capped {
            self.flush_batch();
            let _ = self.log.entry(self.stream, message);
            self.capped = true;
        }
        self.line.clear();
        self.oversized = false;
    }
    fn flush_line(&mut self) {
        if self.capped {
            self.line.clear();
            self.oversized = false;
            return;
        }
        let text = if self.oversized {
            "[omitted: line exceeds 256 KiB]".into()
        } else {
            String::from_utf8_lossy(&self.line)
        };
        let clean = sanitize(&text, &self.log.0.secrets);
        let additional = clean
            .lines()
            .map(|line| line.len().saturating_add(5))
            .sum::<usize>()
            .max(5);
        // Keep room for a batch header and the single explicit limit marker.
        if self.encoded.saturating_add(additional).saturating_add(640) > self.encoded_limit {
            self.cap("[omitted: encoded log capture limit reached; provider parsing continues]");
            return;
        }
        self.encoded = self.encoded.saturating_add(additional);
        self.batch.push_str(&clean);
        self.batch.push('\n');
        self.line.clear();
        self.oversized = false;
        if self.batch.len() >= 32 * 1024 {
            self.flush_batch();
        }
    }
}
impl<R: Read> Read for LogReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = match self.reader.read(buffer) {
            Ok(count) => count,
            Err(error) => {
                if !self.line.is_empty() || self.oversized {
                    self.flush_line();
                }
                self.flush_batch();
                let _ = self.log.entry("Capture incomplete", self.stream);
                return Err(error);
            }
        };
        if count == 0 {
            if !self.line.is_empty() || self.oversized {
                self.flush_line();
            }
            self.flush_batch();
            return Ok(0);
        }
        for byte in &buffer[..count] {
            self.total = self.total.saturating_add(1);
            if self.capped {
                continue;
            }
            if self.total > self.limit {
                self.cap("[omitted: provider stream exceeded the per-turn capture limit]");
                continue;
            }
            if *byte == b'\n' {
                self.flush_line();
            } else if self.line.len() == 256 * 1024 {
                self.line.clear();
                self.oversized = true;
            } else if !self.oversized {
                self.line.push(*byte);
            }
        }
        self.flush_batch();
        Ok(count)
    }
}
impl<R> Drop for LogReader<R> {
    fn drop(&mut self) {
        if !self.line.is_empty() || self.oversized {
            self.flush_line();
        }
        self.flush_batch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transcript_preserves_gameplay_but_redacts_nested_and_echoed_credentials() {
        let token = "a".repeat(64);
        let session = "11111111-1111-4111-8111-111111111111";
        let source = serde_json::json!({"item":{"type":"mcp_tool_call","tool":"register","arguments":{"token":"arbitrary-secret"},"result":{"content":[{"text":format!("{{\"session_id\":\"{session}\",\"token\":\"nested-secret\",\"ok\":true}}")}]},"text":format!("hello {token} {session}\nBearer very-secret\n```\nfood acquired")},"usage":{"input_tokens":123}}).to_string();
        let clean = sanitize(&source, std::slice::from_ref(&token));
        for secret in [
            &token,
            session,
            "arbitrary-secret",
            "nested-secret",
            "very-secret",
        ] {
            assert!(!clean.contains(secret));
        }
        for retained in [
            "mcp_tool_call",
            "register",
            "food acquired",
            "input_tokens",
            "123",
        ] {
            assert!(clean.contains(retained));
        }
        assert!(serde_json::from_str::<Value>(&clean).is_ok());
        let keyed =
            serde_json::json!({token.clone():"ordinary value",session:"second value"}).to_string();
        let keyed = sanitize(&keyed, std::slice::from_ref(&token));
        assert!(!keyed.contains(&token) && !keyed.contains(session));
        assert!(keyed.contains("ordinary value") && keyed.contains("second value"));
        let collision =
            serde_json::json!({"[redacted key 1]":"ordinaryA", token.clone():"ordinaryB"})
                .to_string();
        let collision = sanitize(&collision, std::slice::from_ref(&token));
        let collision: Value = serde_json::from_str(&collision).unwrap();
        assert_eq!(collision.as_object().unwrap().len(), 2);
        assert_eq!(collision["[redacted key 1]"], "ordinaryA");
        assert!(collision.to_string().contains("ordinaryB"));
    }

    #[test]
    fn compact_secret_assignments_are_scrubbed_without_losing_adjacent_gameplay() {
        let source = "Actor gathered food. token=opaque-private &apiKey=sk-fixture-private; accessToken:jwt-private after rest.\nURL https://local.example/play?sessionKey=key-private&action=gather&api_key=api-private done";
        let clean = sanitize(source, &[]);
        for secret in [
            "opaque-private",
            "sk-fixture-private",
            "jwt-private",
            "key-private",
            "api-private",
        ] {
            assert!(
                !clean.contains(secret),
                "compact credentials must not reach logs"
            );
        }
        for retained in [
            "Actor gathered food.",
            "after rest.",
            "action=gather",
            "done",
        ] {
            assert!(
                clean.contains(retained),
                "adjacent gameplay remains available"
            );
        }
        let nested = serde_json::json!({"message":source,"content":[{"text":serde_json::json!({"message":"Actor rested; oauthToken=oauth-private; finished"}).to_string()}]}).to_string();
        let clean = sanitize(&nested, &[]);
        assert!(!clean.contains("oauth-private") && !clean.contains("opaque-private"));
        assert!(clean.contains("Actor rested") && clean.contains("finished"));
        assert!(serde_json::from_str::<Value>(&clean).is_ok());
    }
    #[test]
    fn capture_handles_split_utf8_final_line_limits_and_unique_files() {
        struct Chunks<'a>(&'a [u8]);
        impl Read for Chunks<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = self.0.len().min(buffer.len()).min(1);
                buffer[..count].copy_from_slice(&self.0[..count]);
                self.0 = &self.0[count..];
                Ok(count)
            }
        }
        let root = std::env::temp_dir().join(format!("npc-log-{}", new_secret().unwrap()));
        let log = SessionLog::open(&root, None, Provider::Codex, "private-token").unwrap();
        let mut reader = log.reader(
            Chunks("hello é private-token\nlast without newline".as_bytes()),
            "CLI stdout",
            1024,
        );
        let mut output = Vec::new();
        reader.read_to_end(&mut output).unwrap();
        drop(reader);
        assert_eq!(
            output,
            "hello é private-token\nlast without newline".as_bytes()
        );
        let mut limited = log.reader("abcdef\nignored".as_bytes(), "CLI stderr", 3);
        limited.read_to_end(&mut Vec::new()).unwrap();
        drop(limited);
        let _other = SessionLog::open(&root, None, Provider::Claude, "private-token").unwrap();
        log.flush(Duration::from_secs(2)).unwrap();
        _other.flush(Duration::from_secs(2)).unwrap();
        let files: Vec<_> = fs::read_dir(root.join("logs"))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(files.len(), 2);
        let all = files
            .iter()
            .map(|f| fs::read_to_string(f.path()).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all.contains("hello é") && all.contains("last without newline"));
        assert!(all.contains("per-turn capture limit"));
        assert!(!all.contains("ignored"));
        assert!(!all.contains("private-token"));
        log.shutdown(Duration::from_secs(2)).unwrap();
        _other.shutdown(Duration::from_secs(2)).unwrap();
        drop(log);
        drop(_other);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_lines_and_hostile_markdown_are_explicit_and_cannot_escape_blocks() {
        let root = std::env::temp_dir().join(format!("npc-log-{}", new_secret().unwrap()));
        let log = SessionLog::open(&root, None, Provider::Codex, "secret").unwrap();
        let mut input = vec![b'x'; 256 * 1024 + 1];
        input.extend_from_slice(b"\n# forged heading\n```\n<script>unsafe</script>\n");
        let mut reader = log.reader(input.as_slice(), "CLI stdout", 1024 * 1024);
        let mut forwarded = Vec::new();
        reader.read_to_end(&mut forwarded).unwrap();
        assert_eq!(forwarded, input);
        drop(reader);
        log.flush(Duration::from_secs(2)).unwrap();
        let path = fs::read_dir(root.join("logs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("[omitted: line exceeds 256 KiB]"));
        for line in ["# forged heading", "```", "<script>unsafe</script>"] {
            assert!(text.contains(&format!("\n    {line}\n")));
            assert!(!text.contains(&format!("\n{line}\n")));
        }
        log.shutdown(Duration::from_secs(2)).unwrap();
        drop(log);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn log_write_failure_latches_and_status_still_records_the_error() {
        let root = std::env::temp_dir().join(format!("npc-log-{}", new_secret().unwrap()));
        let log = SessionLog::open(&root, None, Provider::Codex, "secret").unwrap();
        let source = root.join("read-only-handle");
        fs::write(&source, "owned fixture").unwrap();
        log.fail_writes_for_test(&source);
        assert!(
            log.entry("Failure fixture", "must not be silently lost")
                .is_err()
        );
        let _ = log.shutdown(Duration::from_secs(2));
        assert!(log.check().is_err());
        let mut status: storage::Status = serde_json::from_value(serde_json::json!({
            "version":1,"pid":std::process::id(),"provider":"codex","state":"running","message":"fixture",
            "turns_completed":0,"failures":0,"session_ready":false,"connected":false,"updated_unix_seconds":0
        })).unwrap();
        status.log = Some(log.clone());
        assert!(status.set(&root, "error", IO_ERROR).is_err());
        let saved: storage::Status = storage::read_json(&root.join("status.json")).unwrap();
        assert_eq!(saved.state, "error");
        assert_eq!(saved.message, IO_ERROR);
        assert!(
            !fs::read_to_string(root.join("status.json"))
                .unwrap()
                .contains("read-only-handle")
        );
        drop(log);
        drop(status);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_history_indexes_only_owned_runs_and_metrics_across_output_folder_changes() {
        use testbed_content::explorer::ai_history::{self, EntryKind};
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!("npc-log-history-{}", new_secret().unwrap()));
        fs::create_dir(&root).unwrap();
        let private = root.join(".ai-client");
        let first = root.join("shared first output");
        let second = root.join("shared second output");
        let metrics_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let log =
            SessionLog::open(&private, Some(&first), Provider::Codex, "private-token").unwrap();
        log.flush(Duration::from_secs(2)).unwrap();
        let metrics = serde_json::json!({"version":2,"metrics_id":metrics_id,"provider":"codex","totals":{"input_tokens":100,"output_tokens":30},"model":null,"effort":null});
        fs::write(
            first.join(format!("{metrics_id}.json")),
            metrics.to_string(),
        )
        .unwrap();
        log.link_metrics(&first, metrics_id).unwrap();
        log.entry("Owned gameplay", "first run retained").unwrap();
        log.shutdown(Duration::from_secs(2)).unwrap();
        drop(log);
        let log =
            SessionLog::open(&private, Some(&second), Provider::Codex, "private-token").unwrap();
        log.flush(Duration::from_secs(2)).unwrap();
        fs::write(
            second.join(format!("{metrics_id}.json")),
            metrics.to_string(),
        )
        .unwrap();
        log.link_metrics(&second, metrics_id).unwrap();
        log.entry("Owned gameplay", "second run retained").unwrap();
        log.flush(Duration::from_secs(2)).unwrap();
        fs::write(
            second.join("1700000000-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.md"),
            "# AI agent game client session\nOTHER_AGENT_PRIVATE",
        )
        .unwrap();
        let history = ai_history::list(&root).unwrap();
        assert_eq!(
            history
                .entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Log)
                .count(),
            2
        );
        assert_eq!(
            history
                .entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Metrics)
                .count(),
            1
        );
        let mut captured = String::new();
        for entry in history
            .entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::Log)
        {
            assert_eq!(
                entry.metrics_key.as_deref(),
                Some(format!("metrics:{metrics_id}").as_str())
            );
            captured.push_str(&ai_history::read(&root, &entry.key, 0).unwrap().text);
        }
        assert!(
            captured.contains("first run retained") && captured.contains("second run retained")
        );
        assert!(!captured.contains("OTHER_AGENT_PRIVATE") && !captured.contains("private-token"));
        assert!(
            ai_history::read(&root, &format!("metrics:{metrics_id}"), 0)
                .unwrap()
                .text
                .contains("input tokens: 100")
        );
        log.shutdown(Duration::from_secs(2)).unwrap();
        drop(log);
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(&base) && resolved != base);
        fs::remove_dir_all(resolved).unwrap();
    }

    #[test]
    fn newline_flood_has_a_bounded_encoded_log_and_one_explicit_capture_marker() {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!("npc-log-amplification-{}", new_secret().unwrap()));
        let log = SessionLog::open(&root, None, Provider::Codex, "private-token").unwrap();
        let source = vec![b'\n'; 64 * 1024];
        let mut reader = log.reader(source.as_slice(), "CLI stdout", source.len());
        let mut forwarded = Vec::new();
        reader.read_to_end(&mut forwarded).unwrap();
        drop(reader);
        assert_eq!(
            forwarded, source,
            "capture limits do not change parser bytes"
        );
        log.shutdown(Duration::from_secs(2)).unwrap();
        let path = fs::read_dir(root.join("logs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let captured = fs::read_to_string(path).unwrap();
        assert!(captured.len() <= source.len() * 2 + 64 * 1024 + 2048);
        assert_eq!(
            captured
                .matches("encoded log capture limit reached")
                .count(),
            1
        );
        assert!(
            captured.matches("## CLI stdout").count() < 64,
            "stream output is batched, not synchronized once per newline"
        );
        drop(log);
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(&base) && resolved != base);
        fs::remove_dir_all(resolved).unwrap();
    }
}
