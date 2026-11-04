use super::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub const DIR: &str = ".ai-client";
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    pub provider: Provider,
    pub controller: String,
    pub unit: u32,
    pub session: Option<String>,
    #[serde(default)]
    pub metrics_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Status {
    #[serde(skip)]
    pub log: Option<super::session_log::SessionLog>,
    pub version: u32,
    pub pid: u32,
    pub provider: Provider,
    pub state: String,
    pub message: String,
    pub turns_completed: u64,
    pub failures: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure_code: Option<super::diagnostics::FailureCode>,
    pub session_ready: bool,
    pub connected: bool,
    #[serde(default)]
    pub actor_controlled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_connection_fault: Option<super::diagnostics::ConnectionFault>,
    pub updated_unix_seconds: u64,
}
impl Status {
    pub fn set(&mut self, dir: &Path, state: &str, message: &str) -> Result<(), String> {
        let logged = self.log.as_ref().map_or(Ok(()), |log| {
            if self.state != state || self.message != message {
                log.entry(
                    "Client status",
                    &json!({"state":state,"message":message}).to_string(),
                )
            } else {
                log.check()
            }
        });
        self.state = state.into();
        self.message = message.into();
        self.updated_unix_seconds = now();
        // A failed transcript must still leave an actionable, sanitized public status.
        let written = write_json(&dir.join("status.json"), self);
        written.and(logged)
    }
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let file = fs::File::open(path).map_err(|_| "cannot read client workspace file")?;
    let metadata = file
        .metadata()
        .map_err(|_| "cannot inspect client workspace file")?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 {
        return Err("client workspace file is invalid or too large".into());
    }
    let mut bytes = Vec::new();
    file.take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read client workspace file")?;
    if bytes.len() > 256 * 1024 {
        return Err("client workspace file exceeds 256 KiB".into());
    }
    serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
        .map_err(|_| "client workspace JSON is invalid".into())
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "cannot encode client state")?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let temporary = path.with_extension(format!("{}.{stamp}.tmp", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "cannot write private client state")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "cannot flush client state")?;
    fs::rename(&temporary, path).map_err(|_| "cannot replace client state".into())
}

pub struct Lock {
    _file: fs::File,
}
impl Lock {
    pub fn acquire(dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(dir).map_err(|_| "cannot create private client directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
                .map_err(|_| "cannot protect private client directory")?;
        }
        let path = dir.join("lock");
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|_| "cannot open client ownership lock")?;
        file.try_lock().map_err(
            |_| "AI client is already running for this workspace; use Stop before starting another",
        )?;
        file.set_len(0)
            .map_err(|_| "cannot update client ownership lock")?;
        if writeln!(file, "{}", std::process::id()).is_err() {
            return Err("cannot write client lock".into());
        }
        Ok(Self { _file: file })
    }
}

pub fn status(workspace: &Path) -> Result<Value, String> {
    let path = workspace.join(DIR).join("status.json");
    if !path.exists() {
        return Ok(
            json!({"version":1,"state":"stopped","message":"AI client has not been started","connected":false,"actor_controlled":false,"session_ready":false,"turns_completed":0,"failures":0}),
        );
    }
    let mut status: Status = read_json(&path)?;
    if !testbed_content::explorer::host::pid_alive(status.pid)
        && !matches!(status.state.as_str(), "stopped" | "error")
    {
        status.state = "error".into();
        status.message = "AI client exited unexpectedly; Start resumes its saved session after the game lease expires".into();
        status.connected = false;
        status.actor_controlled = false;
    }
    serde_json::to_value(status).map_err(|_| "cannot encode client status".into())
}
pub fn stop(workspace: &Path) -> Result<Value, String> {
    let dir = workspace.join(DIR);
    let active = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("lock"))
        .is_ok_and(|file| file.try_lock().is_err());
    if !active {
        return Ok(json!({"stopping":false,"message":"AI client is already stopped"}));
    }
    fs::write(dir.join("stop"), b"stop\n").map_err(|_| "cannot request AI client stop")?;
    Ok(json!({"stopping":true,"message":"Stopping AI client; the game continues running"}))
}
