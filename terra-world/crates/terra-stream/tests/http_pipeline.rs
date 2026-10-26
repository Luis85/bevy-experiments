//! A local HTTP fixture exercises actual requests, image decoding, clipping,
//! source attribution and caching without depending on any public API uptime.
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use terra_core::{GeoPosition, TileKey};
use terra_stream::{StreamConfig, fetch_weather, load_chunk};

struct FixtureServer {
    address: std::net::SocketAddr,
    requests: Arc<AtomicUsize>,
    overpass_times: Arc<Mutex<Vec<(Instant, Instant)>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl FixtureServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let should_stop = stop.clone();
        let counter = requests.clone();
        let overpass_times = Arc::new(Mutex::new(Vec::new()));
        let times = overpass_times.clone();
        let mut png = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::from_pixel(256, 256, Rgb([128, 10, 0])))
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        let worker = thread::spawn(move || {
            while !should_stop.load(Ordering::Relaxed) {
                let (mut socket, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => break,
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = vec![0u8; 16384];
                let n = socket.read(&mut request).unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&request[..n]);
                let path = request.split_whitespace().nth(1).unwrap_or("");
                let is_overpass = path.starts_with("/overpass");
                let arrived = Instant::now();
                let body: &[u8] = if path.starts_with("/terrain/") {
                    &png
                } else if path.starts_with("/weather") {
                    include_bytes!("fixtures/weather.json")
                } else {
                    include_bytes!("fixtures/overpass.json")
                };
                counter.fetch_add(1, Ordering::Relaxed);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(header.as_bytes());
                if is_overpass {
                    // Keep the response in flight after its headers. The
                    // process-wide gate must cover the complete body as well.
                    thread::sleep(Duration::from_millis(250));
                }
                let _ = socket.write_all(body);
                if is_overpass {
                    times.lock().unwrap().push((arrived, Instant::now()));
                }
            }
        });
        Self {
            address,
            requests,
            overpass_times,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

#[test]
fn http_sources_are_decoded_attributed_clipped_and_cached() {
    let server = FixtureServer::start();
    let directory = std::env::temp_dir().join(format!(
        "terra-http-test-{}-{}",
        std::process::id(),
        server.address.port()
    ));
    let config = StreamConfig {
        live: true,
        allow_fallback: false,
        cache_dir: directory.clone(),
        terrain_url: format!("http://{}/terrain", server.address),
        overpass_url: format!("http://{}/overpass", server.address),
        weather_url: format!("http://{}/weather", server.address),
        ..Default::default()
    };
    let position = GeoPosition::new(48.1005, 11.5005, 0.0).unwrap();
    let key = TileKey::at(position, 16).unwrap();
    let chunk = load_chunk(key, &config).unwrap();
    chunk.validate().unwrap();
    assert_eq!(chunk.heights.min_max(), Some((10.0, 10.0)));
    assert_eq!(chunk.sources.len(), 2);
    assert!(chunk.sources.iter().all(|s| !s.synthetic));
    assert!(!chunk.features.is_empty());
    let bounds = key.bounds();
    assert!(
        chunk
            .features
            .iter()
            .flat_map(|f| &f.points)
            .all(|p| p.longitude >= bounds.west - 1e-10
                && p.longitude <= bounds.east + 1e-10
                && p.latitude >= bounds.south - 1e-10
                && p.latitude <= bounds.north + 1e-10)
    );
    let requests = server.requests.load(Ordering::Relaxed);
    let second = load_chunk(key, &config).unwrap();
    assert_eq!(second, chunk);
    assert_eq!(server.requests.load(Ordering::Relaxed), requests);
    let weather = fetch_weather(position, &config).unwrap();
    assert!(!weather.synthetic);
    assert_eq!(fetch_weather(position, &config).unwrap(), weather);
    assert_eq!(server.requests.load(Ordering::Relaxed), requests + 1);

    // Different requested chunks must share one global Overpass gate even when
    // called from independent worker threads. No public API is contacted here.
    let loads: Vec<_> = (1..=2)
        .map(|offset| {
            let config = config.clone();
            thread::spawn(move || {
                load_chunk(
                    TileKey {
                        x: key.x + offset,
                        ..key
                    },
                    &config,
                )
                .unwrap()
            })
        })
        .collect();
    for load in loads {
        load.join().unwrap();
    }
    let times = server.overpass_times.lock().unwrap().clone();
    assert_eq!(times.len(), 3);
    for pair in times.windows(2) {
        assert!(pair[1].0.duration_since(pair[0].1) >= Duration::from_millis(1900));
    }
    let limited = StreamConfig {
        overpass_request_budget: 1,
        ..config
    };
    let error = load_chunk(
        TileKey {
            x: key.x + 3,
            ..key
        },
        &limited,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("session request budget exhausted")
    );
    assert_eq!(server.overpass_times.lock().unwrap().len(), 3);
    std::fs::remove_dir_all(directory).unwrap();
}
