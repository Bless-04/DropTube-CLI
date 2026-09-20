//! Integration tests for DropTube HTTP server routes and static services.

use droptube::models::state::AppState;
use droptube::server::create_router;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};

struct ServerFixture {
    root: PathBuf,
    port: u16,
}

impl ServerFixture {
    async fn start() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "droptube-server-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("create isolated server test directory");
        let canonical_root = root.canonicalize().expect("canonical root path");

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().expect("local address").port();

        let state = AppState {
            movie_directory: canonical_root.clone(),
            port,
            depth: 255,
            index_cache: Arc::new(RwLock::new(Vec::new())),
            thumbnails: None,
            scan_lock: Arc::new(Mutex::new(())),
        };

        let router = create_router(state);
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        Self {
            root: canonical_root,
            port,
        }
    }

    fn create_file(&self, relative: &str, content: &[u8]) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directory");
        }
        fs::write(&path, content).expect("write mock file");
        path
    }

    async fn request(&self, method: &str, path: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", self.port))
            .await
            .expect("connect to server");

        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        );
        stream.write_all(req.as_bytes()).await.expect("send request");

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.expect("read response");
        let text = String::from_utf8_lossy(&response).to_string();

        let status_code = text
            .lines()
            .next()
            .and_then(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.get(1).and_then(|code| code.parse::<u16>().ok())
            })
            .unwrap_or(0);

        (status_code, text)
    }
}

impl Drop for ServerFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn server_serves_home_feed_and_video_cards() {
    let server = ServerFixture::start().await;
    server.create_file("action_movie.mp4", b"video data");

    // Refresh index so the video appears
    let (refresh_status, _) = server.request("POST", "/refresh").await;
    assert_eq!(refresh_status, 200);

    let (status, body) = server.request("GET", "/").await;
    assert_eq!(status, 200);
    assert!(body.contains("action movie"));
}

#[tokio::test]
async fn server_serves_file_explorer() {
    let server = ServerFixture::start().await;
    server.create_file("courses/rust_intro.mp4", b"video content");

    let (status, body) = server.request("GET", "/explorer").await;
    assert_eq!(status, 200);
    assert!(body.contains("courses"));

    let (sub_status, sub_body) = server.request("GET", "/explorer/courses").await;
    assert_eq!(sub_status, 200);
    assert!(sub_body.contains("rust_intro.mp4"));
}

#[tokio::test]
async fn server_serves_raw_video_via_stream_service() {
    let server = ServerFixture::start().await;
    server.create_file("stream_test.mp4", b"binary-video-payload-12345");

    let (status, body) = server.request("GET", "/video/stream_test.mp4").await;
    assert_eq!(status, 200);
    assert!(body.contains("binary-video-payload-12345"));
}

#[tokio::test]
async fn server_serves_embedded_static_assets() {
    let server = ServerFixture::start().await;

    let (status, body) = server.request("GET", "/public/index.css").await;
    assert_eq!(status, 200);
    assert!(body.contains("Content-Type:") || body.contains("content-type:"));
}

#[tokio::test]
async fn server_handles_missing_files_with_not_found() {
    let server = ServerFixture::start().await;

    let (status, _) = server.request("GET", "/video/non_existent_file.mp4").await;
    assert_eq!(status, 404);

    let (exp_status, _) = server.request("GET", "/explorer/missing_folder").await;
    assert_eq!(exp_status, 404);
}
