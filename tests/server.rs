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
    cookie_name: String,
}

struct TestRequest<'a> {
    method: &'a str,
    path: &'a str,
    headers: &'a [(&'a str, &'a str)],
    body: &'a str,
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

        let profiles = Arc::new(
            droptube::profiles::ProfileManager::new(&canonical_root)
                .await
                .expect("profile manager"),
        );
        let cookie_name = profiles.cookie_name().to_owned();
        let state = AppState {
            movie_directory: canonical_root.clone(),
            port,
            depth: 255,
            index_cache: Arc::new(RwLock::new(Vec::new())),
            thumbnails: None,
            scan_lock: Arc::new(Mutex::new(())),
            profiles,
        };

        let router = create_router(state);
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        Self {
            root: canonical_root,
            port,
            cookie_name,
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
        self.send(TestRequest {
            method,
            path,
            headers: &[],
            body: "",
        })
        .await
    }

    async fn send(&self, request: TestRequest<'_>) -> (u16, String) {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", self.port))
            .await
            .expect("connect to server");

        let mut req = format!(
            "{} {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nContent-Length: {}\r\n",
            request.method,
            request.path,
            self.port,
            request.body.len()
        );
        for (name, value) in request.headers {
            req.push_str(&format!("{name}: {value}\r\n"));
        }
        req.push_str("\r\n");
        req.push_str(request.body);
        stream
            .write_all(req.as_bytes())
            .await
            .expect("send request");

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .expect("read response");
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

fn response_json(response: &str) -> serde_json::Value {
    let (_, body) = response.split_once("\r\n\r\n").expect("HTTP response body");
    serde_json::from_str(body).expect("valid JSON response")
}

#[tokio::test]
async fn profiles_keep_personal_data_isolated_and_replace_community_contributions() {
    let server = ServerFixture::start().await;
    server.create_file("movies/interstellar.mkv", b"video");
    server.create_file("[5] [action,drama] Movie_Name.mp4", b"video");
    assert_eq!(server.request("POST", "/refresh").await.0, 200);
    assert!(!server.root.join(".droptube/profiles").exists());

    let (status, response) = server.request("GET", "/api/videos/ratings").await;
    assert_eq!(status, 200);
    let ratings = response_json(&response);
    assert_eq!(
        ratings["ratings"]["movies/interstellar.mkv"],
        serde_json::json!({"average": null, "count": 0})
    );
    assert_eq!(
        ratings["ratings"]["[5] [action,drama] Movie_Name.mp4"]["count"],
        0
    );

    for (profile, rating) in [("p1", 4), ("p2", 5)] {
        let body =
            serde_json::json!({"video_id":"movies/interstellar.mkv", "rating":rating}).to_string();
        let (status, response) = server
            .send(TestRequest {
                method: "POST",
                path: "/api/user/rating",
                headers: &[
                    ("Content-Type", "application/json"),
                    ("X-Droptube-Profile", profile),
                ],
                body: &body,
            })
            .await;
        assert_eq!(status, 200, "{response}");
        assert_eq!(
            response_json(&response)["ratings"]["movies/interstellar.mkv"],
            rating
        );
        let persisted = fs::read(
            server
                .root
                .join(format!(".droptube/profiles/{profile}/metadata.json")),
        )
        .expect("committed profile JSON");
        let persisted: serde_json::Value =
            serde_json::from_slice(&persisted).expect("complete persisted JSON");
        assert_eq!(persisted["ratings"]["movies/interstellar.mkv"], rating);
    }
    let (_, response) = server.request("GET", "/api/videos/ratings").await;
    assert_eq!(
        response_json(&response)["ratings"]["movies/interstellar.mkv"],
        serde_json::json!({"average":4.5,"count":2})
    );

    for tag in ["Favorites", " sci-fi ", "Favorites"] {
        let body = serde_json::json!({"video_id":"movies/interstellar.mkv", "tag":tag}).to_string();
        let (status, response) = server
            .send(TestRequest {
                method: "POST",
                path: "/api/user/tag",
                headers: &[
                    ("Content-Type", "application/json"),
                    ("X-Droptube-Profile", "p1"),
                ],
                body: &body,
            })
            .await;
        assert_eq!(status, 200, "{response}");
    }
    let (status, response) = server
        .send(TestRequest {
            method: "GET",
            path: "/api/user/metadata",
            headers: &[("X-Droptube-Profile", "p1")],
            body: "",
        })
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        response_json(&response)["tags"]["movies/interstellar.mkv"]
            .as_array()
            .expect("tags")
            .len(),
        2
    );
    let (_, response) = server
        .send(TestRequest {
            method: "GET",
            path: "/api/user/metadata",
            headers: &[("X-Droptube-Profile", "p2")],
            body: "",
        })
        .await;
    assert_eq!(response_json(&response)["tags"], serde_json::json!({}));

    let body = r#"{"video_id":"movies/interstellar.mkv","tag":"FAVORITES"}"#;
    let (status, response) = server
        .send(TestRequest {
            method: "DELETE",
            path: "/api/user/tag",
            headers: &[
                ("Content-Type", "application/json"),
                ("X-Droptube-Profile", "p1"),
            ],
            body,
        })
        .await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(
        response_json(&response)["tags"]["movies/interstellar.mkv"],
        serde_json::json!(["sci-fi"])
    );

    let body = r#"{"video_id":"movies/interstellar.mkv","rating":2}"#;
    for _ in 0..2 {
        assert_eq!(
            server
                .send(TestRequest {
                    method: "POST",
                    path: "/api/user/rating",
                    headers: &[
                        ("Content-Type", "application/json"),
                        ("X-Droptube-Profile", "p1")
                    ],
                    body,
                })
                .await
                .0,
            200
        );
    }
    let (_, response) = server.request("GET", "/api/videos/ratings").await;
    assert_eq!(
        response_json(&response)["ratings"]["movies/interstellar.mkv"],
        serde_json::json!({"average":3.5,"count":2})
    );
    assert!(!response.contains("p1") && !response.contains("sci-fi"));

    let cookie = format!("{}=p2", server.cookie_name);
    let (status, response) = server
        .send(TestRequest {
            method: "GET",
            path: "/?v=movies%2Finterstellar.mkv",
            headers: &[("Cookie", &cookie)],
            body: "",
        })
        .await;
    assert_eq!(status, 200);
    assert!(
        response.contains("3.5/5")
            && response.contains("2 ratings")
            && response.contains("Your rating")
    );
    let (_, response) = server.request("GET", "/").await;
    assert!(response.contains("[5] [action,drama] Movie Name"));
}

#[tokio::test]
async fn browsing_unions_committed_tags_across_profiles_without_changing_edit_ownership() {
    let server = ServerFixture::start().await;
    let ids: Vec<_> = (0..31)
        .map(|index| format!("video{index:02}.mp4"))
        .collect();
    for id in &ids {
        server.create_file(id, b"video");
    }
    let first_tags: serde_json::Map<_, _> = ids
        .iter()
        .map(|id| (id.clone(), serde_json::json!(["favorites"])))
        .collect();
    server.create_file(
        ".droptube/profiles/p1/metadata.json",
        serde_json::json!({"ratings":{},"tags":first_tags})
            .to_string()
            .as_bytes(),
    );
    server.create_file(
        ".droptube/profiles/p2/metadata.json",
        serde_json::json!({"ratings":{},"tags":{
            "video00.mp4":["favorites","<script>shared</script>"],
            "video30.mp4":["late"]
        }})
        .to_string()
        .as_bytes(),
    );
    assert_eq!(server.request("POST", "/refresh").await.0, 200);
    let (status, page) = server.request("GET", "/?tag=favorites&page=2").await;
    assert_eq!(status, 200);
    assert!(page.contains("31 videos") && page.contains("Page 2 of 2"));
    assert_eq!(page.matches("<article class=\"video-card").count(), 7);
    let (_, filtered) = server.request("GET", "/?search=late&tag=late").await;
    assert!(filtered.contains("1 video") && filtered.contains("video30.mp4"));
    let p1 = [("X-Droptube-Profile", "p1")];
    let (_, watch) = server
        .send(TestRequest {
            method: "GET",
            path: "/?v=video00.mp4",
            headers: &p1,
            body: "",
        })
        .await;
    assert!(watch.contains("All tags") && watch.contains("Your tags"));
    assert!(!watch.contains("<script>shared</script>"));
    assert!(watch.contains("data-remove-tag=\"favorites\""));
    assert!(!watch.contains("data-remove-tag=\"&#60;script&#62;shared"));
    let (status, response) = server
        .send(TestRequest {
            method: "DELETE",
            path: "/api/user/tag",
            headers: &[
                ("Content-Type", "application/json"),
                ("X-Droptube-Profile", "p1"),
            ],
            body: r#"{"video_id":"video00.mp4","tag":"favorites"}"#,
        })
        .await;
    assert_eq!(status, 200, "{response}");
    assert!(
        response_json(&response)["tags"]
            .get("video00.mp4")
            .is_none()
    );
    let (_, watch) = server
        .send(TestRequest {
            method: "GET",
            path: "/?v=video00.mp4&tag=favorites",
            headers: &p1,
            body: "",
        })
        .await;
    assert!(watch.contains("class=\"shared-tag\">favorites</span>"));
    assert!(!watch.contains("data-remove-tag=\"favorites\""));
    let (_, p2) = server
        .send(TestRequest {
            method: "GET",
            path: "/api/user/metadata",
            headers: &[("X-Droptube-Profile", "p2")],
            body: "",
        })
        .await;
    assert_eq!(
        response_json(&p2)["tags"]["video00.mp4"]
            .as_array()
            .expect("p2 tags")
            .len(),
        2
    );
    server.create_file(".droptube/profiles/p2/metadata.json", b"{broken");
    let (_, unavailable) = server
        .send(TestRequest {
            method: "GET",
            path: "/?v=video00.mp4&tag=favorites",
            headers: &p1,
            body: "",
        })
        .await;
    assert!(unavailable.contains("Tag results unavailable"));
    assert!(unavailable.contains("Rating unavailable"));
    assert!(unavailable.contains("name=\"tag\" value=\"favorites\""));
    assert!(unavailable.contains("id=\"video-player\""));
    assert!(!unavailable.contains("Personal metadata unavailable"));
    let (_, unavailable_feed) = server.request("GET", "/?tag=favorites").await;
    assert!(unavailable_feed.contains("<h2>Tag results unavailable</h2>"));
    assert!(!unavailable_feed.contains("No videos found"));
    let (_, title_browsing) = server.request("GET", "/?search=video00").await;
    assert!(title_browsing.contains("1 video"));
}

#[tokio::test]
async fn cookie_selection_and_stale_tab_preconditions_do_not_retarget_edits() {
    let server = ServerFixture::start().await;
    server.create_file("movie.mp4", b"video");
    assert_eq!(server.request("POST", "/refresh").await.0, 200);
    let (status, response) = server
        .send(TestRequest {
            method: "POST",
            path: "/api/user/profile",
            headers: &[("Content-Type", "application/json")],
            body: r#"{"profile":"Bob"}"#,
        })
        .await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(response_json(&response)["profile"], "bob");
    assert!(
        response
            .to_lowercase()
            .contains(&format!("set-cookie: {}=bob", server.cookie_name))
    );
    assert!(!server.root.join(".droptube/profiles").exists());

    let cookie = format!("{}=bob", server.cookie_name);
    let (status, response) = server
        .send(TestRequest {
            method: "POST",
            path: "/api/user/rating",
            headers: &[("Content-Type", "application/json"), ("Cookie", &cookie)],
            body: r#"{"video_id":"movie.mp4","rating":4,"expected_profile":"alice"}"#,
        })
        .await;
    assert_eq!(status, 409, "{response}");
    assert!(!server.root.join(".droptube/profiles").exists());
    let (status, _) = server
        .send(TestRequest {
            method: "POST",
            path: "/api/user/rating",
            headers: &[
                ("Content-Type", "application/json"),
                ("X-Droptube-Profile", "../../etc"),
            ],
            body: r#"{"video_id":"movie.mp4","rating":4}"#,
        })
        .await;
    assert_eq!(status, 400);
    assert!(!server.root.join(".droptube/profiles").exists());
}

#[tokio::test]
async fn protected_profile_routes_block_committed_and_temporary_data_without_blocking_media() {
    let server = ServerFixture::start().await;
    server.create_file(
        ".droptube/profiles/alice/metadata.json",
        br#"{"ratings":{},"tags":{}}"#,
    );
    server.create_file(".droptube/profiles/alice/save.tmp", b"private");
    server.create_file(".droptube/thumbnails/movie.mp4.jpg", b"thumbnail");
    server.create_file("movie.mp4", b"media");
    for route in [
        "/video/.droptube/profiles/alice/metadata.json",
        "/video/%2Edroptube%2Fprofiles%2Falice%2Fsave.tmp",
        "/video/.DROPTUBE/PROFILES/alice/metadata.json",
        "/explorer/.droptube/profiles/alice",
        "/explorer/%2Edroptube%2Fprofiles%2Falice%2Fmetadata.json",
    ] {
        let (status, response) = server.request("GET", route).await;
        assert!(
            status == 404 || status == 403 || status == 400,
            "{route}: {response}"
        );
        assert!(!response.contains("private") && !response.contains("{\"ratings\""));
    }
    assert_eq!(
        server
            .request("GET", "/video/.droptube/thumbnails/movie.mp4.jpg")
            .await
            .0,
        200
    );
    assert_eq!(server.request("GET", "/video/movie.mp4").await.0, 200);
}

#[tokio::test]
async fn server_serves_home_feed_and_video_cards() {
    let server = ServerFixture::start().await;
    server.create_file("action_movie.mp4", b"video data");

    // Refresh index so the video appears
    let (refresh_status, _) = server.request("POST", "/refresh").await;
    assert_eq!(
        refresh_status, 200,
        "POST /refresh should return 200 OK after scanning newly added video file"
    );

    let (status, body) = server.request("GET", "/").await;
    assert_eq!(
        status, 200,
        "GET / (home feed) should return 200 OK when requesting root route"
    );
    assert!(
        body.contains("action movie"),
        "Home feed HTML body should contain the formatted video card title 'action movie'"
    );
}

#[tokio::test]
async fn server_serves_file_explorer() {
    let server = ServerFixture::start().await;
    server.create_file("courses/rust_intro.mp4", b"video content");

    let (status, body) = server.request("GET", "/explorer").await;
    assert_eq!(
        status, 200,
        "GET /explorer should return 200 OK when viewing directory structure"
    );
    assert!(
        body.contains("courses"),
        "Root explorer HTML should list the discovered subfolder 'courses'"
    );

    let (sub_status, sub_body) = server.request("GET", "/explorer/courses").await;
    assert_eq!(
        sub_status, 200,
        "GET /explorer/courses should return 200 OK when viewing folder contents"
    );
    assert!(
        sub_body.contains("rust_intro.mp4"),
        "Explorer subfolder HTML should list the video file 'rust_intro.mp4'"
    );
}

#[tokio::test]
async fn server_serves_raw_video_via_stream_service() {
    let server = ServerFixture::start().await;
    server.create_file("stream_test.mp4", b"binary-video-payload-12345");

    let (status, body) = server.request("GET", "/video/stream_test.mp4").await;
    assert_eq!(
        status, 200,
        "GET /video/<file> should return 200 OK when streaming raw video file"
    );
    assert!(
        body.contains("binary-video-payload-12345"),
        "Video streaming endpoint should deliver the raw file binary payload"
    );
}

#[tokio::test]
async fn server_serves_embedded_static_assets() {
    let server = ServerFixture::start().await;

    let (status, body) = server.request("GET", "/public/index.css").await;
    assert_eq!(
        status, 200,
        "GET /public/index.css should return 200 OK for embedded static stylesheet asset"
    );
    assert!(
        body.contains("Content-Type:") || body.contains("content-type:"),
        "Static asset HTTP response headers should contain Content-Type header"
    );
}

#[tokio::test]
async fn server_handles_missing_files_with_not_found() {
    let server = ServerFixture::start().await;

    let (status, _) = server.request("GET", "/video/non_existent_file.mp4").await;
    assert_eq!(
        status, 404,
        "GET /video/<missing_file> should return 404 Not Found for non-existent video"
    );

    let (exp_status, _) = server.request("GET", "/explorer/missing_folder").await;
    assert_eq!(
        exp_status, 404,
        "GET /explorer/<missing_folder> should return 404 Not Found for non-existent directory"
    );
}
