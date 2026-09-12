//! Creation and platform-specific setup for DropTube data directories.

use crate::droptube_dir;
use std::io;
use std::path::{Path, PathBuf};
use tokio::fs;

/// The name of the generated DropTube data directory.
pub const DROPTUBE_DIRECTORY: &str = droptube_dir!("");

/// Ensures that a `.droptube` directory exists inside `parent`.
///
/// Dot-prefixed directories are hidden by convention on Unix. On Windows, this
/// function also applies the Hidden attribute to `.droptube`, including an
/// existing directory that does not yet have it.
pub async fn prepare_droptube_directory(parent: &Path) -> io::Result<PathBuf> {
    let directory = parent.join(DROPTUBE_DIRECTORY);
    fs::create_dir_all(&directory).await?;
    hide_on_windows(&directory).await?;
    Ok(directory)
}

#[cfg(not(windows))]
async fn hide_on_windows(_directory: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(windows)]
async fn hide_on_windows(directory: &Path) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::process::Command;
    use tokio::time::timeout;

    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if fs::metadata(directory).await?.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
        return Ok(());
    }

    let parent = directory
        .parent()
        .ok_or_else(|| io::Error::other("DropTube directory has no parent"))?;
    let name = directory
        .file_name()
        .ok_or_else(|| io::Error::other("DropTube directory has no name"))?;
    let mut command = Command::new("attrib.exe");
    // attrib does not accept Rust's canonical \\?\ paths. Pass the directory
    // name relative to its parent instead of stripping path prefixes.
    command
        .current_dir(parent)
        .arg("+H")
        .arg(name)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .creation_flags(CREATE_NO_WINDOW);
    let output = timeout(Duration::from_secs(5), command.output())
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "attrib.exe timed out"))?
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "Could not hide DropTube directory '{}': {error}",
                    directory.display()
                ),
            )
        })?;

    if !output.status.success()
        || fs::metadata(directory).await?.file_attributes() & FILE_ATTRIBUTE_HIDDEN == 0
    {
        return Err(io::Error::other(format!(
            "Could not hide DropTube directory '{}': attrib exited with {}. {} {}",
            directory.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[tokio::test]
    async fn prepares_the_droptube_directory_idempotently() {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir().join(format!(
            "droptube-directory-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&parent).await.expect("create test parent");

        let directory = prepare_droptube_directory(&parent)
            .await
            .expect("prepare DropTube directory");
        assert_eq!(directory, parent.join(DROPTUBE_DIRECTORY));
        assert!(fs::metadata(&directory).await.expect("metadata").is_dir());
        assert_eq!(
            prepare_droptube_directory(&parent)
                .await
                .expect("prepare existing DropTube directory"),
            directory
        );

        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            assert_ne!(
                fs::metadata(&directory)
                    .await
                    .expect("metadata")
                    .file_attributes()
                    & 0x2,
                0,
                ".droptube must have the Hidden attribute"
            );
        }

        fs::remove_dir_all(parent)
            .await
            .expect("remove test parent");
    }
}
