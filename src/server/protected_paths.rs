//! Shared media/explorer containment and persistent-profile download protection.
//! Canonical checks reject existing aliases to profiles, while lexical checks also
//! protect nonexistent paths and Windows case/trailing-dot variants.

use crate::models::state::AppState;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use percent_encoding::percent_decode_str;
use std::path::{Component, Path, PathBuf};
use tokio::fs;

fn protected_components(path: &Path) -> bool {
    let mut internal = false;
    for component in path.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        let name = name.to_string_lossy();
        // Windows normalizes trailing dots/spaces and compares these names without case.
        // Applying the same conservative block on Unix cannot expose extra data there.
        let name = name.trim_end_matches(['.', ' ']);
        if internal && name.eq_ignore_ascii_case("profiles") {
            return true;
        }
        internal = name.eq_ignore_ascii_case(".droptube");
    }
    false
}

fn validate_relative(path: &str) -> Result<(), StatusCode> {
    if path.contains('\\') || path.chars().any(char::is_control) {
        return Err(StatusCode::FORBIDDEN);
    }
    // Colons cannot be ordinary Windows filenames and can select NTFS alternate streams.
    #[cfg(windows)]
    if path.contains(':') {
        return Err(StatusCode::FORBIDDEN);
    }
    let path = Path::new(path);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        || protected_components(path)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

/// Checks a path already decoded once by its HTTP boundary without opening its contents.
pub(crate) async fn checked_library_path(
    root: &Path,
    decoded: &str,
) -> Result<PathBuf, StatusCode> {
    validate_relative(decoded)?;
    let candidate = fs::canonicalize(root.join(decoded))
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                StatusCode::NOT_FOUND
            } else {
                tracing::warn!(error = %error, "Could not resolve a requested library path");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if protected_components(relative) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(candidate)
}

/// Wraps ServeDir so range/media delivery remains unchanged after the data guard.
pub(crate) async fn protect_video(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let raw = request.uri().path().trim_start_matches('/');
    let decoded = match percent_decode_str(raw).decode_utf8() {
        Ok(decoded) => decoded,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if let Err(status) = checked_library_path(&state.movie_directory, &decoded).await {
        return status.into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn internal_profile_variants_are_blocked_without_blocking_thumbnails() {
        for path in [
            ".droptube/profiles/alice/metadata.json",
            ".DROPTUBE/PROFILES/alice/metadata.json",
            ".droptube./profiles /alice/metadata.json",
            ".droptube/profiles/alice/.metadata-1.tmp",
            "movies/.droptube/profiles/alice/metadata.json",
            "../.droptube/profiles/alice/metadata.json",
            ".droptube\\profiles\\alice\\metadata.json",
            "/.droptube/profiles/alice/metadata.json",
        ] {
            assert_eq!(
                validate_relative(path),
                Err(StatusCode::FORBIDDEN),
                "{path}"
            );
        }
        for path in [
            "movie.mp4",
            "movies/film.mkv",
            ".droptube/thumbnails/film.mp4.jpg",
            "movies/.droptube/thumbnails/film.mkv.jpg",
        ] {
            assert_eq!(validate_relative(path), Ok(()), "{path}");
        }
    }

    #[test]
    fn encoded_paths_are_checked_after_exactly_one_decode() {
        for raw in [
            "%2edroptube/profiles/alice/metadata.json",
            ".droptube%2fprofiles%2falice%2fmetadata.json",
            "%2EDROPTUBE/%50ROFILES/alice/metadata.json",
        ] {
            let decoded = percent_decode_str(raw).decode_utf8().expect("UTF8 path");
            assert_eq!(validate_relative(&decoded), Err(StatusCode::FORBIDDEN));
        }
        let decoded = percent_decode_str("%252edroptube/profiles/alice/metadata.json")
            .decode_utf8()
            .expect("UTF8 literal percent path");
        assert_eq!(
            validate_relative(&decoded),
            Ok(()),
            "a second decoding layer must not be introduced"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_alternate_streams_and_directory_stream_aliases_are_blocked() {
        for raw in [
            "film.mp4:secret",
            "film.mp4%3asecret",
            ".droptube/profiles::$INDEX_ALLOCATION/alice/metadata.json",
            ".droptube/profiles%3a%3a%24INDEX_ALLOCATION/alice/metadata.json",
            "C:film.mp4",
        ] {
            let decoded = percent_decode_str(raw)
                .decode_utf8()
                .expect("UTF8 stream path");
            assert_eq!(
                validate_relative(&decoded),
                Err(StatusCode::FORBIDDEN),
                "{raw}"
            );
        }
        assert_eq!(validate_relative("film.mp4"), Ok(()));
    }

    #[tokio::test]
    async fn canonical_aliases_cannot_escape_the_library_or_expose_profiles() {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "droptube-protected-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".droptube/profiles/alice"))
            .await
            .expect("create profile fixture");
        fs::write(root.join(".droptube/profiles/alice/metadata.json"), b"{}")
            .await
            .expect("write profile fixture");
        fs::write(root.join("film.mp4"), b"video")
            .await
            .expect("write media fixture");
        let root = fs::canonicalize(root).await.expect("canonical root");
        assert!(checked_library_path(&root, "film.mp4").await.is_ok());
        assert_eq!(
            checked_library_path(&root, ".droptube/profiles/alice/metadata.json").await,
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            checked_library_path(&root, "missing.mp4").await,
            Err(StatusCode::NOT_FOUND)
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join(".droptube/profiles"), root.join("alias"))
                .expect("symlink profile directory");
            assert_eq!(
                checked_library_path(&root, "alias/alice/metadata.json").await,
                Err(StatusCode::FORBIDDEN)
            );
            std::os::unix::fs::symlink(root.parent().expect("root parent"), root.join("outside"))
                .expect("symlink outside directory");
            assert_eq!(
                checked_library_path(&root, "outside").await,
                Err(StatusCode::FORBIDDEN)
            );
        }
        #[cfg(windows)]
        {
            let output = tokio::process::Command::new("cmd.exe")
                .args(["/D", "/C", "mklink", "/J", "alias", ".droptube\\profiles"])
                .current_dir(&root)
                .creation_flags(0x0800_0000)
                .output()
                .await
                .expect("create profile junction");
            assert!(
                output.status.success(),
                "junction creation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                checked_library_path(&root, "alias/alice/metadata.json").await,
                Err(StatusCode::FORBIDDEN)
            );
            fs::remove_dir(root.join("alias"))
                .await
                .expect("remove only the junction");
        }
        fs::remove_dir_all(root)
            .await
            .expect("remove protected fixture");
    }
}
