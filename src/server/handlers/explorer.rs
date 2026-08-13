use crate::models::state::AppState;
use crate::models::video::VideoFormat;
use crate::server::handlers::TemplateError;
use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use local_ip_address::local_ip;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use std::fs;
use std::path::Path as StdPath;
use std::time::SystemTime;

#[derive(Template)]
#[template(path = "explorer.html")]
pub struct ExplorerTemplate {
    pub search_query: String,
    pub breadcrumbs_html: String,
    pub entries_html: String,
    pub local_ip: String,
    pub port: u16,
}

/// Explorer Root routing helper
pub async fn explorer_root_handler(
    State(state): State<AppState>,
) -> Result<Response, TemplateError> {
    explorer_handler(State(state), Path(String::new())).await
}

/// Explorer Subdirectories routing helper
pub async fn explorer_path_handler(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Response, TemplateError> {
    explorer_handler(State(state), Path(path)).await
}

/// Handles generic folder layout rendering and directory browsing
async fn explorer_handler(
    State(state): State<AppState>,
    Path(sub_path): Path<String>,
) -> Result<Response, TemplateError> {
    // Percent-decode the sub-path
    let decoded_sub_path = percent_encoding::percent_decode_str(&sub_path)
        .decode_utf8()
        .unwrap_or_else(|_| std::borrow::Cow::Borrowed(""));

    // Safe path join
    let target_path = state.movie_directory.join(decoded_sub_path.as_ref());
    // Prevent path traversal attack
    if !target_path.starts_with(&state.movie_directory) {
        return Ok((
            StatusCode::FORBIDDEN,
            Html("<h1>403 Forbidden</h1><p>Directory traversal access is denied.</p>".to_string()),
        )
            .into_response());
    }

    //  Check exists
    if !target_path.exists() {
        return Ok((
            StatusCode::NOT_FOUND,
            Html("<h1>404 Not Found</h1><p>File or folder does not exist.</p>".to_string()),
        )
            .into_response());
    }

    // Check if Directory or File
    if target_path.is_dir() {
        let mut entries_html = String::new();

        // Render Back Button if in subdirectory
        if !decoded_sub_path.is_empty() {
            let parent_path = StdPath::new(decoded_sub_path.as_ref())
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
        if let Ok(entries) = fs::read_dir(&target_path) {
            let mut folders = Vec::new();
            let mut files = Vec::new();

            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name.ends_with(".json") {
                    continue; // Skip hidden or sidecar files
                }

                let p = entry.path();
                if p.is_dir() {
                    folders.push(name);
                } else if let Ok(meta) = fs::metadata(&p) {
                    files.push((name, meta.len(), meta.modified()));
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
                    encoded_path, folder
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
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
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
                    encoded_path, file, type_str, size_str, time_str
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
                segment
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
        };

        let full_html = template.render()?;
        Ok(Html(full_html).into_response())
    } else {
        // Redirect to the static /video endpoint
        let encoded_file = utf8_percent_encode(&decoded_sub_path, NON_ALPHANUMERIC).to_string();
        Ok(Redirect::temporary(&format!("/video/{}", encoded_file)).into_response())
    }
}
