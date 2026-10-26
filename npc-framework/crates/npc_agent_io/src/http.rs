//! A minimal JSON-RPC 2.0 over HTTP/1.1 transport (std only), speaking
//! the BRP wire format (`POST /` with a JSON-RPC request or batch), so the
//! same clients talk to `npcctl serve` and to a game's BRP server.
//!
//! **Server** ([`HttpServer`]): one listener thread on `127.0.0.1:<port>`
//! and one short-lived thread per connection (at most [`MAX_CONNECTIONS`];
//! more get `503`). A connection thread parses the request, hands each call
//! to the host as an [`Incoming`] over a channel and blocks *its own* thread
//! until the host replies; the host (the session loop) drains the channel
//! without ever blocking. So a long-poll (`agent/events {timeout_ms}`) parks
//! a connection thread, never the game loop. Responses close the connection
//! (`Connection: close`).
//!
//! **Request checks** (a browser page can reach a localhost port: CSRF with
//! a "simple" `text/plain` POST, or DNS rebinding; `docs/debugging.md`
//! §4, "Transport security"). Before the body is read, a request is refused
//! when:
//! - it is not `POST` (`405`);
//! - it carries an `Origin` header (`403`): browsers send one on every
//!   cross-origin and every `POST` request, CLI/MCP/script clients do not;
//! - its `Host` is not `127.0.0.1:<port>`, `localhost:<port>` or
//!   `[::1]:<port>` (`421`): a rebound DNS name arrives with its own name;
//! - a token is configured ([`ServerOptions::token`], `npcctl serve
//!   --token`) and `Authorization: Bearer <token>` does not match (`401`);
//! - `Content-Type` is not `application/json` (`415`): a cross-site "simple"
//!   request cannot set it without a CORS preflight, which this server never
//!   answers;
//! - there is no `Content-Length` (`411`), a `Transfer-Encoding` (`501`) or
//!   a body over [`MAX_BODY`] (`413`).
//!
//! **Limits**: the request line and headers are read through a
//! [`MAX_HEAD`]-byte window (`431` beyond), the whole request must arrive
//! within [`REQUEST_DEADLINE`] (a slowly dripped request is dropped), a
//! batch has at most [`MAX_BATCH`] calls, and an empty batch is a JSON-RPC
//! `-32600` error. Every refusal is a JSON-RPC error object with a message
//! naming the rule; the server then reads the connection until the client
//! closes it (at most a second), so a reset cannot destroy the answer. One
//! thread answers the `503`s, so the listener never waits on a client.
//!
//! **Client** ([`RpcClient`]): one connection per call; reads
//! `Content-Length` or chunked bodies (hyper's, for BRP servers); sends
//! `Authorization: Bearer` when given a token ([`RpcClient::with_token`]).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::api::{RpcError, RpcResult, codes};
use crate::stats::wall_now;

/// Largest request body accepted (`413` beyond).
pub const MAX_BODY: usize = 1024 * 1024;
/// Largest request line + headers (`431` beyond).
pub const MAX_HEAD: u64 = 64 * 1024;
/// Most connections served at once (`503` beyond).
pub const MAX_CONNECTIONS: usize = 64;
/// Most calls in one batch.
pub const MAX_BATCH: usize = 64;
/// The whole request (head and body) must arrive within this.
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(10);
/// How long a refused connection may take to close after its answer.
const REFUSE_LINGER: Duration = Duration::from_secs(1);
/// Connections over the cap waiting for their `503` (dropped beyond).
const BUSY_QUEUE: usize = MAX_CONNECTIONS;
/// How long a connection waits for the host beyond a call's own timeout.
pub const REPLY_GRACE: Duration = Duration::from_secs(30);
/// How long an `npc/step` call may take (up to a million ticks).
pub const STEP_WAIT: Duration = Duration::from_secs(600);

/// How long one call may take before its answer (server: waiting for the
/// host; client: the socket read): a call's own `timeout_ms` plus
/// [`REPLY_GRACE`] (`agent/act_and_wait`: its default and cap,
/// [`crate::api::ACT_WAIT_DEFAULT_MS`] / [`crate::api::ACT_WAIT_MAX_MS`],
/// apply), and [`STEP_WAIT`] for `npc/step` (a long step runs in slices on
/// the session host and answers when done).
pub fn call_wait(method: &str, params: Option<&Value>) -> Duration {
    if method == "npc/step" {
        return STEP_WAIT;
    }
    let given = params
        .and_then(|p| p.get("timeout_ms"))
        .and_then(Value::as_u64);
    let ms = if method == "agent/act_and_wait" {
        given
            .unwrap_or(crate::api::ACT_WAIT_DEFAULT_MS)
            .min(crate::api::ACT_WAIT_MAX_MS)
    } else {
        given.unwrap_or(0)
    };
    Duration::from_millis(ms).saturating_add(REPLY_GRACE)
}

/// Server settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerOptions {
    /// Require `Authorization: Bearer <token>` on every request.
    pub token: Option<String>,
}

/// One call waiting for the host.
pub struct Incoming {
    pub method: String,
    pub params: Option<Value>,
    reply: Sender<RpcResult>,
}

impl Incoming {
    /// Sends the answer (a dropped client is fine).
    pub fn reply(self, result: RpcResult) {
        let _ = self.reply.send(result);
    }
}

/// The server side: bind, then drain [`HttpServer::try_recv`] from the host
/// loop. Dropping it stops the listener.
pub struct HttpServer {
    rx: Receiver<Incoming>,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl HttpServer {
    /// Listens on `127.0.0.1:port` (`0`: any free port), without a token.
    pub fn bind(port: u16) -> io::Result<Self> {
        Self::bind_with(port, ServerOptions::default())
    }

    /// Listens on `127.0.0.1:port` with `options`.
    pub fn bind_with(port: u16, options: ServerOptions) -> io::Result<Self> {
        Self::bind_counted(port, options, REQUEST_DEADLINE).map(|(server, _)| server)
    }

    /// [`Self::bind_with`] with its own request deadline; also returns the
    /// count of connections holding a slot (tests watch it).
    fn bind_counted(
        port: u16,
        options: ServerOptions,
        request_deadline: Duration,
    ) -> io::Result<(Self, Arc<AtomicUsize>)> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        let addr = listener.local_addr()?;
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = Arc::clone(&stop);
        let active = Arc::new(AtomicUsize::new(0));
        let active2 = Arc::clone(&active);
        let conf = Arc::new(Conf {
            port: addr.port(),
            token: options.token.filter(|t| !t.is_empty()),
            request_deadline,
        });
        let (busy_tx, busy_rx) = mpsc::sync_channel(BUSY_QUEUE);
        thread::Builder::new()
            .name("npc_agent_io-busy".into())
            .spawn(move || refuse_busy(&busy_rx))?;
        thread::Builder::new()
            .name("npc_agent_io-http".into())
            .spawn(move || accept_loop(&listener, &tx, &busy_tx, &stop2, &active2, &conf))?;
        Ok((Self { rx, addr, stop }, active))
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// The next waiting call, if any (never blocks).
    pub fn try_recv(&self) -> Option<Incoming> {
        self.rx.try_recv().ok()
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
    }
}

/// What every connection thread checks against.
struct Conf {
    port: u16,
    token: Option<String>,
    request_deadline: Duration,
}

/// Decrements the connection count when a connection thread ends.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn accept_loop(
    listener: &TcpListener,
    tx: &Sender<Incoming>,
    busy: &SyncSender<TcpStream>,
    stop: &AtomicBool,
    active: &Arc<AtomicUsize>,
    conf: &Arc<Conf>,
) {
    for conn in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = conn else { continue };
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::SeqCst);
            // Answered off this thread (a refusal lingers); a full queue
            // drops the connection unanswered rather than stall accepting.
            let _ = busy.try_send(stream);
            continue;
        }
        let slot = Slot(Arc::clone(active));
        let tx = tx.clone();
        let conf = Arc::clone(conf);
        let spawned = thread::Builder::new()
            .name("npc_agent_io-conn".into())
            .spawn(move || {
                let _slot = slot;
                if let Err(e) = handle_connection(stream, &tx, &conf) {
                    tracing::debug!(target: "npc_agent_io", "connection: {e}");
                }
            });
        if let Err(e) = spawned {
            tracing::debug!(target: "npc_agent_io", "cannot spawn a connection thread: {e}");
        }
    }
}

/// Answers the connections over [`MAX_CONNECTIONS`] with `503`, one at a
/// time; ends when the accept loop does.
fn refuse_busy(busy: &Receiver<TcpStream>) {
    for mut stream in busy {
        let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
        let _ = refuse(
            &mut stream,
            "503 Service Unavailable",
            &format!("too many connections (at most {MAX_CONNECTIONS} at once); retry later"),
        );
    }
}

/// A socket read that gives up at `deadline` (each read gets the time left
/// as its timeout), so a dripped request cannot hold a thread forever.
struct DeadlineReader {
    inner: TcpStream,
    deadline: Instant,
}

impl Read for DeadlineReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self
            .deadline
            .checked_duration_since(wall_now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "request deadline passed"))?;
        self.inner.set_read_timeout(Some(left))?;
        self.inner.read(buf)
    }
}

/// The request line and headers (names lower case).
struct Head {
    method: String,
    headers: Vec<(String, String)>,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Why a request is refused: HTTP status and message.
type Refusal = (&'static str, String);

/// Reads the head through a [`MAX_HEAD`] window. `Ok(Err(..))`: answer
/// with that refusal; `Err`: the connection broke (no answer).
fn read_head(reader: &mut impl BufRead) -> io::Result<Result<Head, Refusal>> {
    let mut window = (&mut *reader).take(MAX_HEAD);
    let mut line = String::new();
    if window.read_line(&mut line)? == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "no request"));
    }
    let too_large = || {
        (
            "431 Request Header Fields Too Large",
            format!("request line and headers exceed {MAX_HEAD} bytes"),
        )
    };
    if !line.ends_with('\n') {
        return Ok(Err(too_large()));
    }
    let method = line.split_whitespace().next().unwrap_or("").to_owned();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        let n = window.read_line(&mut h)?;
        if n == 0 {
            if window.limit() == 0 {
                return Ok(Err(too_large()));
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "headers cut off",
            ));
        }
        if !h.ends_with('\n') {
            return Ok(Err(too_large()));
        }
        if h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
        }
    }
    Ok(Ok(Head { method, headers }))
}

/// Constant-time string comparison (for the token).
fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The checks of the module docs, in order; `Ok(body length)`.
fn check_head(head: &Head, conf: &Conf) -> Result<usize, Refusal> {
    if head.method != "POST" {
        return Err((
            "405 Method Not Allowed",
            "POST a JSON-RPC 2.0 request to /".into(),
        ));
    }
    if head.header("origin").is_some() {
        return Err((
            "403 Forbidden",
            "requests with an Origin header are refused: browser pages may not call this \
             server (use npcctl, the MCP server or a script)"
                .into(),
        ));
    }
    let host = head.header("host").unwrap_or("").to_ascii_lowercase();
    let port = conf.port;
    let allowed = [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ];
    if !allowed.contains(&host) {
        return Err((
            "421 Misdirected Request",
            format!(
                "Host `{host}` is refused: expected 127.0.0.1:{port}, localhost:{port} or \
                 [::1]:{port}"
            ),
        ));
    }
    if let Some(token) = &conf.token {
        let given = head
            .header("authorization")
            .and_then(|v| {
                v.strip_prefix("Bearer ")
                    .or_else(|| v.strip_prefix("bearer "))
            })
            .map(str::trim);
        if !given.is_some_and(|g| same(g, token)) {
            return Err((
                "401 Unauthorized",
                "this session needs `Authorization: Bearer <token>` (npcctl: --token)".into(),
            ));
        }
    }
    let json_type = head.header("content-type").is_some_and(|v| {
        v.split(';')
            .next()
            .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"))
    });
    if !json_type {
        return Err((
            "415 Unsupported Media Type",
            "Content-Type must be application/json".into(),
        ));
    }
    if head.header("transfer-encoding").is_some() {
        return Err((
            "501 Not Implemented",
            "Transfer-Encoding is not supported; send Content-Length".into(),
        ));
    }
    let len = head
        .header("content-length")
        .ok_or_else(|| {
            (
                "411 Length Required",
                "Content-Length is required".to_owned(),
            )
        })?
        .parse::<usize>()
        .map_err(|_| ("400 Bad Request", "bad Content-Length".to_owned()))?;
    if len > MAX_BODY {
        return Err((
            "413 Content Too Large",
            format!("request body over {MAX_BODY} bytes"),
        ));
    }
    Ok(len)
}

fn write_response(stream: &mut TcpStream, status: &str, body: &str) -> io::Result<()> {
    let auth = if status.starts_with("401") {
        "WWW-Authenticate: Bearer\r\n"
    } else {
        ""
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()?;
    let _ = stream.shutdown(Shutdown::Write);
    Ok(())
}

/// Answers with `status` and a JSON-RPC error (`-32600`) naming the rule,
/// then reads what the client still sends until it closes (at most
/// [`REFUSE_LINGER`]): closing a socket with unread input resets the
/// connection (Windows), and the reset can destroy the answer before the
/// client reads it.
fn refuse(stream: &mut TcpStream, status: &str, message: &str) -> io::Result<()> {
    let err = error_response(
        Value::Null,
        &RpcError::new(-32600, format!("{status}: {message}")),
    );
    write_response(stream, status, &err.to_string())?;
    let mut rest = DeadlineReader {
        inner: stream.try_clone()?,
        deadline: wall_now() + REFUSE_LINGER,
    }
    .take(MAX_HEAD + MAX_BODY as u64);
    let _ = io::copy(&mut rest, &mut io::sink());
    Ok(())
}

fn error_response(id: Value, err: &RpcError) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": err})
}

/// Handles one JSON-RPC request object; `None` for a notification.
fn handle_call(req: &Value, tx: &Sender<Incoming>) -> Option<Value> {
    let id = req.get("id").cloned();
    let Some(method) = req.get("method").and_then(Value::as_str) else {
        return Some(error_response(
            id.unwrap_or(Value::Null),
            &RpcError::new(-32600, "invalid request: no method"),
        ));
    };
    let params = req.get("params").cloned();
    let timeout = call_wait(method, params.as_ref());
    let (reply, rx) = mpsc::channel();
    let sent = tx.send(Incoming {
        method: method.to_owned(),
        params,
        reply,
    });
    let result = match sent {
        Err(_) => Err(RpcError::new(
            codes::INTERNAL,
            "the session is shutting down",
        )),
        Ok(()) => rx
            .recv_timeout(timeout)
            .unwrap_or_else(|_| Err(RpcError::new(codes::INTERNAL, "no reply from the session"))),
    };
    let id = id?;
    Some(match result {
        Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
        Err(e) => error_response(id, &e),
    })
}

/// The response to a parsed body (`None`: only notifications, no body).
fn respond(parsed: Result<Value, serde_json::Error>, tx: &Sender<Incoming>) -> Option<Value> {
    match parsed {
        Err(e) => Some(error_response(
            Value::Null,
            &RpcError::new(-32700, format!("parse error: {e}")),
        )),
        Ok(Value::Array(calls)) if calls.is_empty() => Some(error_response(
            Value::Null,
            &RpcError::new(-32600, "invalid request: empty batch"),
        )),
        Ok(Value::Array(calls)) if calls.len() > MAX_BATCH => Some(error_response(
            Value::Null,
            &RpcError::new(
                -32600,
                format!(
                    "invalid request: batch of {} calls (at most {MAX_BATCH})",
                    calls.len()
                ),
            ),
        )),
        Ok(Value::Array(calls)) => {
            let out: Vec<Value> = calls.iter().filter_map(|c| handle_call(c, tx)).collect();
            (!out.is_empty()).then_some(Value::Array(out))
        }
        Ok(call) => handle_call(&call, tx),
    }
}

fn handle_connection(stream: TcpStream, tx: &Sender<Incoming>, conf: &Conf) -> io::Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(DeadlineReader {
        inner: stream.try_clone()?,
        deadline: wall_now() + conf.request_deadline,
    });
    let mut stream = stream;
    let head = match read_head(&mut reader)? {
        Ok(head) => head,
        Err((status, message)) => return refuse(&mut stream, status, &message),
    };
    let len = match check_head(&head, conf) {
        Ok(len) => len,
        Err((status, message)) => return refuse(&mut stream, status, &message),
    };
    let mut body = Vec::with_capacity(len.min(64 * 1024));
    (&mut reader).take(len as u64).read_to_end(&mut body)?;
    if body.len() != len {
        return refuse(
            &mut stream,
            "400 Bad Request",
            "body shorter than Content-Length",
        );
    }
    match respond(serde_json::from_slice(&body), tx) {
        Some(r) => write_response(&mut stream, "200 OK", &r.to_string()),
        None => write_response(&mut stream, "200 OK", ""),
    }
}

// ---------------------------------------------------------------- client

/// Why a call failed.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientError {
    /// Connection, HTTP or JSON trouble.
    Transport(String),
    /// The server answered with a JSON-RPC error.
    Rpc(RpcError),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Transport(e) => write!(f, "transport error: {e}"),
            ClientError::Rpc(e) => write!(f, "rpc error {}: {}", e.code, e.message),
        }
    }
}

impl std::error::Error for ClientError {}

/// A stateless JSON-RPC client (`http://host:port[/path]`).
pub struct RpcClient {
    host: String,
    port: u16,
    path: String,
    token: Option<String>,
    next_id: AtomicU64,
    /// Socket timeout beyond a call's own `timeout_ms`.
    pub timeout: Duration,
}

impl RpcClient {
    /// Parses `url` (`http://127.0.0.1:15702`).
    pub fn new(url: &str) -> Result<Self, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("url `{url}` must start with http://"))?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].to_owned()),
            None => (rest, "/".to_owned()),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (
                h.to_owned(),
                p.parse::<u16>()
                    .map_err(|_| format!("bad port in url `{url}`"))?,
            ),
            None => (authority.to_owned(), 80),
        };
        if host.is_empty() {
            return Err(format!("no host in url `{url}`"));
        }
        Ok(Self {
            host,
            port,
            path,
            token: None,
            next_id: AtomicU64::new(1),
            timeout: Duration::from_secs(30),
        })
    }

    /// Sends `Authorization: Bearer <token>` with every call (a session
    /// started with `npcctl serve --token`); `None` or empty: no header.
    #[must_use]
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token.filter(|t| !t.is_empty());
        self
    }

    /// `http://host:port/path`.
    pub fn url(&self) -> String {
        format!("http://{}:{}{}", self.host, self.port, self.path)
    }

    /// Calls `method` with `params` and returns its `result`.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let wait = call_wait(method, Some(&params));
        let body =
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        let text = self
            .post(&body, wait.max(self.timeout))
            .map_err(|e| ClientError::Transport(format!("{} ({e})", self.url())))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| {
            ClientError::Transport(format!(
                "{}: response is not JSON ({e}): {text}",
                self.url()
            ))
        })?;
        if let Some(err) = v.get("error") {
            let err: RpcError = serde_json::from_value(err.clone())
                .unwrap_or_else(|_| RpcError::new(codes::INTERNAL, err.to_string()));
            return Err(ClientError::Rpc(err));
        }
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    }

    fn post(&self, body: &str, timeout: Duration) -> io::Result<String> {
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot resolve host"))?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
        stream.set_read_timeout(Some(timeout))?;
        let auth = self
            .token
            .as_ref()
            .map_or_else(String::new, |t| format!("Authorization: Bearer {t}\r\n"));
        write!(
            stream,
            "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.path,
            self.host,
            self.port,
            body.len()
        )?;
        stream.flush()?;
        let mut reader = BufReader::new(stream);
        let mut status = String::new();
        reader.read_line(&mut status)?;
        let mut headers = Vec::new();
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                headers.push((k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase()));
            }
        }
        let header = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        let body = if header("transfer-encoding").is_some_and(|v| v.contains("chunked")) {
            read_chunked(&mut reader)?
        } else if let Some(len) = header("content-length").and_then(|v| v.parse::<usize>().ok()) {
            let mut b = vec![0; len];
            reader.read_exact(&mut b)?;
            b
        } else {
            let mut b = Vec::new();
            reader.read_to_end(&mut b)?;
            b
        };
        let code = status.split_whitespace().nth(1).unwrap_or("");
        let text = String::from_utf8_lossy(&body).into_owned();
        if !code.starts_with('2') && text.is_empty() {
            return Err(io::Error::other(format!("HTTP {}", status.trim())));
        }
        Ok(text)
    }
}

fn read_chunked(reader: &mut impl BufRead) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let mut size = String::new();
        reader.read_line(&mut size)?;
        let n = usize::from_str_radix(size.trim().split(';').next().unwrap_or("0"), 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad chunk size"))?;
        if n == 0 {
            break;
        }
        let mut chunk = vec![0; n];
        reader.read_exact(&mut chunk)?;
        out.extend_from_slice(&chunk);
        let mut crlf = String::new();
        reader.read_line(&mut crlf)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_a_host_loop() {
        let server = HttpServer::bind(0).expect("bind");
        let client = RpcClient::new(&server.url()).expect("url");
        let done = Arc::new(AtomicBool::new(false));
        let done2 = Arc::clone(&done);
        let host = thread::spawn(move || {
            while !done2.load(Ordering::SeqCst) {
                while let Some(call) = server.try_recv() {
                    let echo = json!({"method": call.method, "params": call.params});
                    if call.method == "fail" {
                        call.reply(Err(RpcError::invalid("nope")));
                    } else {
                        call.reply(Ok(echo));
                    }
                }
                thread::sleep(Duration::from_millis(1));
            }
        });
        let r = client.call("echo", json!({"a": 1})).expect("call");
        assert_eq!(r["method"], "echo");
        assert_eq!(r["params"]["a"], 1);
        let e = client.call("fail", json!({})).expect_err("error");
        assert!(
            matches!(e, ClientError::Rpc(RpcError { code: -32602, .. })),
            "{e:?}"
        );
        done.store(true, Ordering::SeqCst);
        host.join().expect("host");
    }

    /// An echo host loop on its own thread; stops when the server is dropped
    /// by the returned guard.
    struct Echo {
        port: u16,
        /// Connections holding a slot.
        active: Arc<AtomicUsize>,
        done: Arc<AtomicBool>,
        host: Option<thread::JoinHandle<()>>,
    }

    impl Drop for Echo {
        fn drop(&mut self) {
            self.done.store(true, Ordering::SeqCst);
            if let Some(h) = self.host.take() {
                let _ = h.join();
            }
        }
    }

    fn echo(options: ServerOptions) -> Echo {
        echo_with_deadline(options, REQUEST_DEADLINE)
    }

    fn echo_with_deadline(options: ServerOptions, request_deadline: Duration) -> Echo {
        let (server, active) =
            HttpServer::bind_counted(0, options, request_deadline).expect("bind");
        let port = server.port();
        let done = Arc::new(AtomicBool::new(false));
        let done2 = Arc::clone(&done);
        let host = thread::spawn(move || {
            while !done2.load(Ordering::SeqCst) {
                while let Some(call) = server.try_recv() {
                    let echo = json!({"method": call.method});
                    call.reply(Ok(echo));
                }
                thread::sleep(Duration::from_millis(1));
            }
        });
        Echo {
            port,
            active,
            done,
            host: Some(host),
        }
    }

    /// Sends `request` verbatim; returns (status code, body JSON or Null).
    fn raw(port: u16, request: &str) -> (u16, Value) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        s.write_all(request.as_bytes()).expect("write");
        let mut text = String::new();
        let _ = s.read_to_string(&mut text);
        let code = text
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let body = text.split_once("\r\n\r\n").map_or("", |(_, b)| b);
        (code, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn post(port: u16, extra: &str, body: &str) -> (u16, Value) {
        raw(
            port,
            &format!(
                "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
    }

    const CALL: &str = r#"{"jsonrpc":"2.0","id":1,"method":"m"}"#;

    #[test]
    fn accepts_a_well_formed_request() {
        let e = echo(ServerOptions::default());
        let (code, v) = post(e.port, "", CALL);
        assert_eq!(code, 200, "{v}");
        assert_eq!(v["result"]["method"], "m");
        for host in ["localhost", "LOCALHOST", "[::1]"] {
            let (code, v) = raw(
                e.port,
                &format!(
                    "POST / HTTP/1.1\r\nHost: {host}:{}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\r\n{CALL}",
                    e.port,
                    CALL.len()
                ),
            );
            assert_eq!(code, 200, "{host}: {v}");
        }
    }

    #[test]
    fn refuses_browser_and_rebinding_requests() {
        let e = echo(ServerOptions::default());
        let port = e.port;
        // CSRF from a page: a "simple" request carries Origin (and text/plain).
        let (code, v) = post(port, "Origin: https://evil.example\r\n", CALL);
        assert_eq!(code, 403, "{v}");
        assert!(
            v["error"]["message"].as_str().unwrap().contains("Origin"),
            "{v}"
        );
        let (code, _) = post(port, "Origin: null\r\n", CALL);
        assert_eq!(code, 403);
        // DNS rebinding: the page's own name arrives as Host.
        for host in [
            format!("evil.example:{port}"),
            "127.0.0.1".to_owned(),
            format!("127.0.0.1:{}", port.wrapping_add(1)),
            String::new(),
        ] {
            let (code, v) = raw(
                port,
                &format!(
                    "POST / HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{CALL}",
                    CALL.len()
                ),
            );
            assert_eq!(code, 421, "Host `{host}`: {v}");
        }
        let (code, v) = raw(
            port,
            &format!(
                "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{CALL}",
                CALL.len()
            ),
        );
        assert_eq!(code, 415, "{v}");
        let (code, _) = raw(
            port,
            &format!("POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 2\r\n\r\n{{}}"),
        );
        assert_eq!(code, 415, "no Content-Type");
        let (code, _) = raw(
            port,
            &format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
        );
        assert_eq!(code, 405);
    }

    #[test]
    fn enforces_size_limits() {
        let e = echo(ServerOptions::default());
        let port = e.port;
        let (code, _) = raw(
            port,
            &format!(
                "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\r\n"
            ),
        );
        assert_eq!(code, 411);
        let (code, _) = raw(
            port,
            &format!(
                "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
            ),
        );
        assert_eq!(code, 501);
        // Refused from the header alone: no 2 MiB body is ever sent or allocated.
        let (code, v) = raw(
            port,
            &format!(
                "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                2 * MAX_BODY
            ),
        );
        assert_eq!(code, 413, "{v}");
        let huge = format!(
            "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Pad: {}\r\n\r\n",
            "a".repeat(usize::try_from(MAX_HEAD).unwrap())
        );
        let (code, _) = raw(port, &huge);
        assert_eq!(code, 431);
    }

    #[test]
    fn batches_are_bounded_and_never_empty() {
        let e = echo(ServerOptions::default());
        let (code, v) = post(e.port, "", "[]");
        assert_eq!(code, 200);
        assert_eq!(v["error"]["code"], -32600, "{v}");
        let big = format!("[{}]", vec![CALL; MAX_BATCH + 1].join(","));
        let (_, v) = post(e.port, "", &big);
        assert_eq!(v["error"]["code"], -32600, "{v}");
        let ok = format!("[{}]", [CALL; 3].join(","));
        let (_, v) = post(e.port, "", &ok);
        assert_eq!(v.as_array().map(Vec::len), Some(3), "{v}");
        // Only notifications: no body at all.
        let (code, v) = post(e.port, "", r#"[{"jsonrpc":"2.0","method":"m"}]"#);
        assert_eq!((code, v), (200, Value::Null));
    }

    #[test]
    fn token_is_required_when_configured() {
        let e = echo(ServerOptions {
            token: Some("s3cret".into()),
        });
        let url = format!("http://127.0.0.1:{}", e.port);
        let err = RpcClient::new(&url)
            .unwrap()
            .call("m", json!({}))
            .expect_err("no token");
        assert!(format!("{err}").contains("Bearer"), "{err}");
        let err = RpcClient::new(&url)
            .unwrap()
            .with_token(Some("wrong".into()))
            .call("m", json!({}))
            .expect_err("wrong token");
        assert!(matches!(err, ClientError::Rpc(_)), "{err}");
        let ok = RpcClient::new(&url)
            .unwrap()
            .with_token(Some("s3cret".into()))
            .call("m", json!({}))
            .expect("token");
        assert_eq!(ok["method"], "m");
        let (code, _) = post(e.port, "Authorization: Bearer s3cret\r\n", CALL);
        assert_eq!(code, 200);
        let (code, _) = post(e.port, "", CALL);
        assert_eq!(code, 401);
    }

    /// Waits (by the clock, not a retry count: a loaded machine can take
    /// seconds to schedule 64 connection threads) until `want` slots are held.
    fn wait_for_slots(active: &AtomicUsize, want: usize, what: &str) {
        let deadline = wall_now() + Duration::from_secs(60);
        while active.load(Ordering::SeqCst) != want {
            assert!(
                wall_now() < deadline,
                "{what}: {} slots held after 60 s, want {want}",
                active.load(Ordering::SeqCst)
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn caps_concurrent_connections() {
        // A request deadline far beyond the waits below: a slot that comes
        // back was freed by its connection closing, not by the deadline.
        let e = echo_with_deadline(ServerOptions::default(), Duration::from_secs(3600));
        // Idle connections that never send a byte hold their slot.
        let idle: Vec<TcpStream> = (0..MAX_CONNECTIONS)
            .map(|_| TcpStream::connect(("127.0.0.1", e.port)).expect("connect"))
            .collect();
        wait_for_slots(&e.active, MAX_CONNECTIONS, "idle connections accepted");
        let (code, v) = post(e.port, "", CALL);
        assert_eq!(code, 503, "{v}");
        drop(idle);
        // Slots come back once those connections close.
        wait_for_slots(&e.active, 0, "slots after the idle connections closed");
        let (code, v) = post(e.port, "", CALL);
        assert_eq!(code, 200, "{v}");
    }

    #[test]
    fn transport_errors_name_the_url() {
        let c = RpcClient::new("http://127.0.0.1:9").expect("url");
        let err = c.call("m", json!({})).expect_err("nothing listens on 9");
        assert!(err.to_string().contains("http://127.0.0.1:9/"), "{err}");
    }

    #[test]
    fn client_parses_urls() {
        let c = RpcClient::new("http://127.0.0.1:15702").expect("ok");
        assert_eq!(c.url(), "http://127.0.0.1:15702/");
        assert!(RpcClient::new("127.0.0.1:1").is_err());
        assert!(RpcClient::new("http://h:notaport").is_err());
    }
}
