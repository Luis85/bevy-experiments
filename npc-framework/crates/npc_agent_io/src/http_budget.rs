//! Bounded loopback control traffic, separate from legacy action/event long polls.

use super::*;
use std::net::IpAddr;

const RESPONSE_HEAD_LIMIT: usize = 8192;
const INVALID_CONTROL_RESPONSE: i64 = -32600;

impl RpcClient {
    /// Calls a loopback server within one network deadline and response budget.
    ///
    /// Unlike [`Self::call`], this does not extend the budget for action waits.
    /// Connect, write and every socket read share the same absolute deadline.
    /// `localhost` is resolved directly to loopback; other names are refused so
    /// synchronous DNS cannot defeat the budget. Response headers and chunk
    /// framing have separate 8 KiB limits. No failed request is retried here.
    /// Invalid responses become a closed `client_protocol_mismatch` RPC error;
    /// HTTP authentication refusals become `client_auth_required`. Transport
    /// errors never include response content, credentials or a URL.
    pub fn call_with_budget(
        &self,
        method: &str,
        params: Value,
        budget: Duration,
        max_response_bytes: usize,
    ) -> Result<Value, ClientError> {
        if budget.is_zero() || !(1..=MAX_BODY).contains(&max_response_bytes) {
            return Err(protocol_error());
        }
        let host = self.host.trim_start_matches('[').trim_end_matches(']');
        let ip: IpAddr = if host.eq_ignore_ascii_case("localhost") {
            Ipv4Addr::LOCALHOST.into()
        } else {
            host.parse().map_err(|_| protocol_error())?
        };
        if !ip.is_loopback()
            || !self.path.starts_with('/')
            || self.path.bytes().any(|b| b.is_ascii_control() || b == b' ')
            || self
                .token
                .as_ref()
                .is_some_and(|token| token.bytes().any(|b| b.is_ascii_control()))
        {
            return Err(protocol_error());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string();
        if body.len() > MAX_BODY {
            return Err(protocol_error());
        }
        let deadline = wall_now().checked_add(budget).ok_or_else(protocol_error)?;
        let result = self.post_bounded(&body, ip, deadline, max_response_bytes);
        let (status, bytes) = result.map_err(|error| {
            if error.kind() == io::ErrorKind::InvalidData {
                protocol_error()
            } else {
                ClientError::Transport("game control connection interrupted or timed out".into())
            }
        })?;
        match status {
            200..=299 => {}
            401 | 403 => {
                return Err(ClientError::Rpc(RpcError {
                    code: INVALID_CONTROL_RESPONSE,
                    message: "game transport authentication refused".into(),
                    data: Some(json!({"reason":"client_auth_required"})),
                }));
            }
            429 | 503 => {
                return Err(ClientError::Transport(
                    "game control server temporarily unavailable".into(),
                ));
            }
            _ => return Err(protocol_error()),
        }
        let response: Value = serde_json::from_slice(&bytes).map_err(|_| protocol_error())?;
        let valid = response.is_object()
            && response.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
            && response.get("id").and_then(Value::as_u64) == Some(id)
            && response.get("result").is_some() != response.get("error").is_some();
        if !valid {
            return Err(protocol_error());
        }
        if let Some(error) = response.get("error") {
            return Err(ClientError::Rpc(
                serde_json::from_value(error.clone()).map_err(|_| protocol_error())?,
            ));
        }
        Ok(response["result"].clone())
    }

    fn post_bounded(
        &self,
        body: &str,
        ip: IpAddr,
        deadline: Instant,
        limit: usize,
    ) -> io::Result<(u16, Vec<u8>)> {
        let stream =
            TcpStream::connect_timeout(&SocketAddr::new(ip, self.port), remaining(deadline)?)?;
        let mut stream = BudgetStream { stream, deadline };
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
        let mut head_budget = RESPONSE_HEAD_LIMIT;
        let status = bounded_line(&mut reader, &mut head_budget)?;
        let mut words = status.split_whitespace();
        if !matches!(words.next(), Some("HTTP/1.0" | "HTTP/1.1")) {
            return Err(invalid());
        }
        let code: u16 = words
            .next()
            .ok_or_else(invalid)?
            .parse()
            .map_err(|_| invalid())?;
        let mut length = None;
        let mut chunked = false;
        loop {
            let line = bounded_line(&mut reader, &mut head_budget)?;
            if line.is_empty() {
                break;
            }
            let (name, value) = line.split_once(':').ok_or_else(invalid)?;
            if name.eq_ignore_ascii_case("content-length") {
                if length.is_some() {
                    return Err(invalid());
                }
                length = Some(value.trim().parse::<usize>().map_err(|_| invalid())?);
            } else if name.eq_ignore_ascii_case("transfer-encoding") {
                if chunked || !value.trim().eq_ignore_ascii_case("chunked") {
                    return Err(invalid());
                }
                chunked = true;
            }
        }
        // Refusals do not need their potentially malformed or stalled body.
        if !(200..300).contains(&code) {
            return Ok((code, Vec::new()));
        }
        if chunked && length.is_some() {
            return Err(invalid());
        }
        let mut bytes = Vec::new();
        if chunked {
            let mut framing_budget = RESPONSE_HEAD_LIMIT;
            loop {
                let size = bounded_line(&mut reader, &mut framing_budget)?;
                let count = usize::from_str_radix(size.split(';').next().unwrap_or(""), 16)
                    .map_err(|_| invalid())?;
                if count == 0 {
                    loop {
                        let trailer = bounded_line(&mut reader, &mut framing_budget)?;
                        if trailer.is_empty() {
                            break;
                        }
                        if !trailer.contains(':') {
                            return Err(invalid());
                        }
                    }
                    break;
                }
                if count > limit.saturating_sub(bytes.len()) {
                    return Err(invalid());
                }
                let start = bytes.len();
                bytes.resize(start + count, 0);
                reader.read_exact(&mut bytes[start..])?;
                let mut end = [0; 2];
                reader.read_exact(&mut end)?;
                framing_budget = framing_budget.checked_sub(2).ok_or_else(invalid)?;
                if end != *b"\r\n" {
                    return Err(invalid());
                }
            }
        } else if let Some(count) = length {
            if count > limit {
                return Err(invalid());
            }
            bytes.resize(count, 0);
            reader.read_exact(&mut bytes)?;
        } else {
            reader.take((limit + 1) as u64).read_to_end(&mut bytes)?;
            if bytes.len() > limit {
                return Err(invalid());
            }
        }
        remaining(deadline)?;
        Ok((code, bytes))
    }
}

fn protocol_error() -> ClientError {
    ClientError::Rpc(RpcError {
        code: INVALID_CONTROL_RESPONSE,
        message: "incompatible game control response".into(),
        data: Some(json!({"reason":"client_protocol_mismatch"})),
    })
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded game response")
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(wall_now())
        .filter(|left| !left.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "game control deadline elapsed"))
}

struct BudgetStream {
    stream: TcpStream,
    deadline: Instant,
}

impl Read for BudgetStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.stream
            .set_read_timeout(Some(remaining(self.deadline)?))?;
        self.stream.read(buffer)
    }
}

impl Write for BudgetStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.stream
            .set_write_timeout(Some(remaining(self.deadline)?))?;
        self.stream.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        remaining(self.deadline)?;
        self.stream.flush()
    }
}

fn bounded_line(reader: &mut impl BufRead, budget: &mut usize) -> io::Result<String> {
    let mut bytes = Vec::new();
    reader
        .take((*budget + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if bytes.len() > *budget {
        return Err(invalid());
    }
    if bytes.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "game control response interrupted",
        ));
    }
    *budget -= bytes.len();
    let line = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(
        respond: impl FnOnce(TcpStream) + Send + 'static,
    ) -> (RpcClient, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client = RpcClient::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.strip_prefix("Content-Length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            drop(reader);
            respond(stream);
        });
        (client, handle)
    }

    fn exchange(response: String, limit: usize) -> Result<Value, ClientError> {
        let (client, handle) = server(move |mut stream| {
            let _ = stream.write_all(response.as_bytes());
        });
        let result =
            client.call_with_budget("client/poll", json!({}), Duration::from_secs(2), limit);
        handle.join().unwrap();
        result
    }

    fn fixed(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    fn is_protocol(result: &Result<Value, ClientError>) -> bool {
        matches!(result, Err(ClientError::Rpc(error))
            if error.data.as_ref().and_then(|d| d.get("reason")).and_then(Value::as_str)
                == Some("client_protocol_mismatch"))
    }

    #[test]
    fn bounded_control_round_trips_result_and_structured_rpc_failure() {
        assert_eq!(
            exchange(
                fixed(r#"{"jsonrpc":"2.0","id":1,"result":{"attached":true}}"#),
                1024
            )
            .unwrap(),
            json!({"attached":true})
        );
        let result = exchange(
            fixed(
                r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"denied","data":{"reason":"revoked"}}}"#,
            ),
            1024,
        );
        assert!(
            matches!(result, Err(ClientError::Rpc(error)) if error.data.as_ref().unwrap()["reason"] == "revoked")
        );
    }

    #[test]
    fn bounded_control_checks_response_identity_and_exclusive_envelope() {
        for body in [
            r#"{"jsonrpc":"2.0","id":2,"result":true}"#,
            r#"{"jsonrpc":"1.0","id":1,"result":true}"#,
            r#"{"jsonrpc":"2.0","id":1}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":true,"error":null}"#,
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":"bad"}}"#,
            "private-response-value-not-to-be-reported",
        ] {
            let result = exchange(fixed(body), 1024);
            assert!(is_protocol(&result), "{result:?}");
            assert!(!format!("{result:?}").contains("private-response-value"));
        }
    }

    #[test]
    fn bounded_control_limits_declared_eof_chunked_and_header_memory() {
        for response in [
            "HTTP/1.1 200 OK\r\nContent-Length: 18446744073709551615\r\n\r\n".into(),
            format!(
                "HTTP/1.1 200 OK\r\nX: {}\r\n\r\n",
                "a".repeat(RESPONSE_HEAD_LIMIT)
            ),
            format!("HTTP/1.1 200 OK\r\n\r\n{}", "x".repeat(33)),
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1000000\r\n".into(),
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}".into(),
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n".into(),
        ] {
            let result = exchange(response, 32);
            assert!(is_protocol(&result), "{result:?}");
        }
    }

    #[test]
    fn bounded_control_decodes_chunk_extensions_and_trailers_with_limits() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x};test=1\r\n{body}\r\n0\r\nX-Trailer: done\r\n\r\n",
            body.len()
        );
        assert_eq!(exchange(response, body.len()).unwrap(), Value::Null);
        let response = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX: {}\r\n\r\n",
            "a".repeat(RESPONSE_HEAD_LIMIT)
        );
        assert!(is_protocol(&exchange(response, 1024)));
    }

    #[test]
    fn bounded_control_http_authentication_and_busy_refusals_are_distinct() {
        let auth = exchange(
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 99999\r\n\r\n".into(),
            1024,
        );
        assert!(
            matches!(auth, Err(ClientError::Rpc(error)) if error.data.as_ref().unwrap()["reason"] == "client_auth_required")
        );
        assert!(matches!(
            exchange("HTTP/1.1 503 Busy\r\n\r\n".into(), 1024),
            Err(ClientError::Transport(_))
        ));
    }

    #[test]
    fn bounded_control_interrupted_headers_remain_transient() {
        assert!(matches!(
            exchange("HTTP/1.1 200 OK\r\nContent-Len".into(), 1024),
            Err(ClientError::Transport(_))
        ));
    }

    #[test]
    fn bounded_control_uses_one_deadline_instead_of_the_legacy_wait_floor() {
        let (release, wait) = mpsc::channel();
        let (client, handle) = server(move |_stream| {
            // A broken deadline would wait for this failsafe instead of the 80 ms budget.
            let _ = wait.recv_timeout(Duration::from_secs(3));
        });
        let started = wall_now();
        let result =
            client.call_with_budget("client/claim", json!({}), Duration::from_millis(80), 1024);
        let elapsed = started.elapsed();
        release.send(()).unwrap();
        handle.join().unwrap();
        assert!(matches!(result, Err(ClientError::Transport(_))));
        assert!(
            elapsed < Duration::from_secs(2),
            "deadline took {elapsed:?}"
        );
        assert_eq!(call_wait("client/claim", None), REPLY_GRACE);
        assert!(call_wait("agent/events", Some(&json!({"timeout_ms":5000}))) > REPLY_GRACE);
    }

    #[test]
    fn bounded_control_slow_trickle_cannot_renew_the_read_deadline() {
        let (client, handle) = server(move |mut stream| {
            for byte in fixed(r#"{"jsonrpc":"2.0","id":1,"result":true}"#).bytes() {
                if stream.write_all(&[byte]).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        });
        let started = wall_now();
        let result =
            client.call_with_budget("client/poll", json!({}), Duration::from_millis(100), 1024);
        let elapsed = started.elapsed();
        handle.join().unwrap();
        assert!(matches!(result, Err(ClientError::Transport(_))));
        assert!(
            elapsed < Duration::from_secs(1),
            "trickle lasted {elapsed:?}"
        );
    }

    #[test]
    fn bounded_control_refuses_dns_and_out_of_scope_hosts_without_connecting() {
        for host in ["must-not-resolve.invalid", "192.0.2.1"] {
            let client = RpcClient::new(&format!("http://{host}:1")).unwrap();
            assert!(is_protocol(&client.call_with_budget(
                "client/claim",
                json!({}),
                Duration::from_secs(1),
                1024
            )));
        }
    }
}
