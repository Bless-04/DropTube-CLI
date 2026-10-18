use crate::models::state::AppState;
use crate::models::video::VideoFormat;
use crate::server::handlers::profiles::{ApiError, resolve_profile};
use crate::server::protected_paths::checked_library_path;
use askama::Template;
use axum::{
    extract::{OriginalUri, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use local_ip_address::local_ip;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use std::path::Path as StdPath;
use std::time::SystemTime;
use tokio::fs;

#[derive(Template)]
#[template(path = "explorer.html")]
pub struct ExplorerTemplate {
    pub search_query: String,
    pub breadcrumbs_html: String,
    pub entries_html: String,
    pub local_ip: String,
    pub port: u16,
    pub active_profile: Option<String>,
    pub profile_input: String,
    pub return_to: String,
}

/// Explorer Root routing helper
pub async fn explorer_root_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> Result<Response, ApiError> {
    explorer_handler(state, String::new(), headers, uri.to_string()).await
}

/// Explorer Subdirectories routing helper
pub async fn explorer_path_handler(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> Result<Response, ApiError> {
    explorer_handler(state, path, headers, uri.to_string()).await
}

/// Handles generic folder layout rendering and directory browsing
async fn explorer_handler(
    state: AppState,
    sub_path: String,
    headers: HeaderMap,
    return_to: String,
) -> Result<Response, ApiError> {
    let active_profile = resolve_profile(&headers, &state.profiles)?.map(|profile| profile.name);
    // Axum has already decoded the path once. Canonicalize before checking containment.
    let decoded_sub_path = sub_path;
    let target_path = match checked_library_path(&state.movie_directory, &decoded_sub_path).await {
        Ok(path) => path,
        Err(status) => return Ok(status.into_response()),
    };

    // Check if Directory or File
    if fs::metadata(&target_path)
        .await
        .map_err(explorer_io_error)?
        .is_dir()
    {
        let mut entries_html = String::new();

        // Render Back Button if in subdirectory
        if !decoded_sub_path.is_empty() {
            let parent_path = StdPath::new(&decoded_sub_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let back_url = if parent_path.is_empty() {
                "/explorer".to_string()
            } else {
                format!(
                    "/explorer/{}",
                    utf8_percent_encode(&parent_path, NON_ALPHANUMERIC)
                )
            };

            entries_html.push_str(&format!(
                r#"
                <tr class="border-b border-zinc-800 hover:bg-zinc-900/50 transition-colors">
                    <td class="px-4 py-3 font-semibold text-red-500">
                        <a href="{}" class="flex items-center gap-2 select-none">
                            📁 .. (Go Back)
                        </a>
                    </td>
                    <td class="px-4 py-3 text-zinc-500">-</td>
                    <td class="px-4 py-3 text-zinc-500">-</td>
                    <td class="px-4 py-3 text-zinc-500">-</td>
                </tr>
                "#,
                back_url
            ));
        }

        // Scan directory contents
        {
            let mut entries = fs::read_dir(&target_path)
                .await
                .map_err(explorer_io_error)?;
            let mut folders = Vec::new();
            let mut files = Vec::new();

            while let Some(entry) = entries.next_entry().await.map_err(explorer_io_error)? {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name.ends_with(".json") {
                    continue; // Skip hidden or sidecar files
                }

                let p = entry.path();
                let relative = p
                    .strip_prefix(&state.movie_directory)
                    .map_err(|_| ApiError::internal("Could not browse this directory"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                match checked_library_path(&state.movie_directory, &relative).await {
                    Ok(_) => {}
                    Err(StatusCode::FORBIDDEN) => continue,
                    Err(status) => {
                        tracing::warn!(status = %status, "Could not inspect an explorer entry");
                        return Err(ApiError::internal("Could not browse this directory"));
                    }
                }
                let meta = fs::metadata(&p).await.map_err(explorer_io_error)?;
                if meta.is_dir() {
                    folders.push(name);
                } else {
                    files.push((
                        name,
                        meta.len(),
                        meta.modified().map_err(explorer_io_error)?,
                    ));
                }
            }

            folders.sort_by_key(|a| a.to_lowercase());
            files.sort_by_key(|(a, _, _)| a.to_lowercase());

            // Render directory folders
            for folder in folders {
                let relative_folder_path = if decoded_sub_path.is_empty() {
                    folder.clone()
                } else {
                    format!("{}/{}", decoded_sub_path, folder)
                };
                let encoded_path =
                    utf8_percent_encode(&relative_folder_path, NON_ALPHANUMERIC).to_string();

                entries_html.push_str(&format!(
                    r#"
                    <tr class="border-b border-zinc-800 hover:bg-zinc-900/50 transition-colors">
                        <td class="px-4 py-3 font-medium text-amber-500">
                            <a href="/explorer/{}" class="flex items-center gap-2">
                                📁 {}/
                            </a>
                        </td>
                        <td class="px-4 py-3 text-zinc-400">Directory</td>
                        <td class="px-4 py-3 text-zinc-400">-</td>
                        <td class="px-4 py-3 text-zinc-500">Folder</td>
                    </tr>
                    "#,
                    encoded_path,
                    escape_html(&folder)
                ));
            }

            // Render directory files
            for (file, size, modified) in files {
                let relative_file_path = if decoded_sub_path.is_empty() {
                    file.clone()
                } else {
                    format!("{}/{}", decoded_sub_path, file)
                };
                let encoded_path =
                    utf8_percent_encode(&relative_file_path, NON_ALPHANUMERIC).to_string();

                let size_str = if size >= 1024 * 1024 * 1024 {
                    format!("{:.2} GB", (size as f64) / (1024.0 * 1024.0 * 1024.0))
                } else if size >= 1024 * 1024 {
                    format!("{:.2} MB", (size as f64) / (1024.0 * 1024.0))
                } else {
                    format!("{:.1} KB", (size as f64) / 1024.0)
                };

                let ext = StdPath::new(&file)
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_lowercase())
                    .unwrap_or_default();

                let type_str = if VideoFormat::SUPPORTED_EXTS.contains(&ext.as_str()) {
                    format!("Video ({})", ext)
                } else {
                    ext.clone()
                };

                let time_str = modified
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
                    .map(|sec| {
                        format!(
                            r#"<span class="time-elapsed" data-timestamp="{}"></span>"#,
                            sec
                        )
                    })
                    .unwrap_or_else(|| "-".to_string());

                entries_html.push_str(&format!(
                    r#"
                    <tr class="border-b border-zinc-800 hover:bg-zinc-900/50 transition-colors">
                        <td class="px-4 py-3 font-medium text-zinc-300">
                            <a href="/video/{}" target="_parent" class="hover:text-red-500 flex items-center gap-2">
                                📄 {}
                            </a>
                        </td>
                        <td class="px-4 py-3 text-zinc-400 capitalize">{}</td>
                        <td class="px-4 py-3 text-zinc-400">{}</td>
                        <td class="px-4 py-3 text-zinc-400">{}</td>
                    </tr>
                    "#,
                    encoded_path, escape_html(&file), escape_html(&type_str), size_str, time_str
                ));
            }
        }

        // Construct breadcrumb navigation links
        let mut breadcrumbs_html =
            r#"<a href="/explorer" class="text-zinc-500 hover:text-red-500">root</a>"#.to_string();
        let mut accumulated = String::new();
        for segment in decoded_sub_path.split('/') {
            if segment.is_empty() {
                continue;
            }
            if !accumulated.is_empty() {
                accumulated.push('/');
            }
            accumulated.push_str(segment);
            breadcrumbs_html.push_str(&format!(
                r#" <span class="text-zinc-700">/</span> <a href="/explorer/{}" class="text-zinc-300 hover:text-red-500">{}</a>"#,
                utf8_percent_encode(&accumulated, NON_ALPHANUMERIC),
                escape_html(segment)
            ));
        }

        let local_ip_addr = local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "0.0.0.0".to_string());

        let template = ExplorerTemplate {
            search_query: String::new(),
            breadcrumbs_html,
            entries_html,
            local_ip: local_ip_addr,
            port: state.port,
            profile_input: active_profile.clone().unwrap_or_else(|| "guest".to_owned()),
            active_profile,
            return_to,
        };

        let full_html = template
            .render()
            .map_err(crate::server::handlers::TemplateError)?;
        Ok(Html(full_html).into_response())
    } else {
        // Redirect to the static /video endpoint
        let encoded_file = utf8_percent_encode(&decoded_sub_path, NON_ALPHANUMERIC).to_string();
        Ok(Redirect::temporary(&format!("/video/{}", encoded_file)).into_response())
    }
}

fn explorer_io_error(error: std::io::Error) -> ApiError {
    tracing::error!(error = %error, "Could not read an explorer directory");
    ApiError::internal("Could not browse this directory")
}

// Explorer rows still use HTML fragments; escape filesystem labels before rendering them.
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::sync::{Mutex, RwLock};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(prefix: &str) -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "droptube-{prefix}-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create test dir");
            Self(path.canonicalize().expect("canonicalize test dir"))
        }

        fn create_file(&self, relative: &str, content: &[u8]) -> PathBuf {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dirs");
            }
            fs::write(&path, content).expect("write mock file");
            path
        }

        async fn state(&self) -> AppState {
            AppState {
                movie_directory: self.0.clone(),
                port: 8081,
                depth: 255,
                index_cache: Arc::new(RwLock::new(Vec::new())),
                thumbnails: None,
                scan_lock: Arc::new(Mutex::new(())),
                profiles: Arc::new(
                    crate::profiles::ProfileManager::new(&self.0)
                        .await
                        .expect("profile manager"),
                ),
            }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn filesystem_labels_are_escaped_before_inserting_html() {
        assert_eq!(
            super::escape_html("<b>O'Brien & \"friends\"</b>"),
            "&lt;b&gt;O&#39;Brien &amp; &quot;friends&quot;&lt;/b&gt;",
            "HTML special characters (<, >, ', &, \") should be escaped to safe entities"
        );
    }

    #[tokio::test]
    async fn explorer_root_handler_renders_directory_contents() {
        let fixture = TestDir::new("exp-root");
        fixture.create_file("subfolder/video.mp4", b"video");
        fixture.create_file("top_level.mp4", b"video");

        let response = explorer_root_handler(
            State(fixture.state().await),
            HeaderMap::new(),
            OriginalUri("/explorer".parse().expect("URI")),
        )
        .await
        .expect("render explorer root")
        .into_response();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "explorer_root_handler should return HTTP 200 OK"
        );
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let html = String::from_utf8_lossy(&body);

        assert!(
            html.contains("subfolder"),
            "Explorer root HTML should list the subfolder name"
        );
        assert!(
            html.contains("top_level.mp4"),
            "Explorer root HTML should list the top-level video file"
        );
    }

    #[tokio::test]
    async fn explorer_path_handler_renders_subfolder_with_back_button() {
        let fixture = TestDir::new("exp-sub");
        fixture.create_file("my_folder/inner.mp4", b"video");

        let response = explorer_path_handler(
            State(fixture.state().await),
            axum::extract::Path("my_folder".to_string()),
            HeaderMap::new(),
            OriginalUri("/explorer/my_folder".parse().expect("URI")),
        )
        .await
        .expect("render subfolder")
        .into_response();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "explorer_path_handler should return HTTP 200 OK for valid subfolder"
        );
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let html = String::from_utf8_lossy(&body);

        assert!(
            html.contains("inner.mp4"),
            "Subfolder explorer HTML should list the inner video file"
        );
        assert!(
            html.contains("Go Back"),
            "Subfolder explorer HTML should display a 'Go Back' navigation link"
        );
    }

    #[tokio::test]
    async fn explorer_path_handler_returns_not_found_for_missing_path() {
        let fixture = TestDir::new("exp-missing");

        let response = explorer_path_handler(
            State(fixture.state().await),
            axum::extract::Path("non_existent_folder".to_string()),
            HeaderMap::new(),
            OriginalUri("/explorer/non_existent_folder".parse().expect("URI")),
        )
        .await
        .expect("handler returns response")
        .into_response();

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "explorer_path_handler should return HTTP 404 NOT_FOUND for non-existent path"
        );
    }

    #[tokio::test]
    async fn unavailable_filesystem_timestamps_return_a_controlled_error_without_internal_details()
    {
        let response = explorer_io_error(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "could not obtain timestamp for private-library-path",
        ))
        .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), 1024)
            .await
            .expect("error response");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("JSON error");
        assert_eq!(
            value,
            serde_json::json!({"error":"Could not browse this directory"})
        );
    }
}
