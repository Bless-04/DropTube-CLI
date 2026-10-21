//! Bounded, read-only profile snapshots kept off the terminal's render/input loop.

use crate::config::logger::TuiLogControl;
use crate::models::state::AppState;
use crate::profiles::ProfileSummary;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// Shared application data and logging used by the profile-aware terminal entry point.
///
/// Creating this context does not read or write profile files. The terminal starts one
/// asynchronous reader and displays only committed contribution counts.
pub struct ProfileTuiContext {
    pub(super) state: AppState,
    pub(super) log_control: TuiLogControl,
}

impl ProfileTuiContext {
    /// Connects the terminal to the same index, profile locks, and logs as the web server.
    #[must_use]
    pub fn new(state: AppState, log_control: TuiLogControl) -> Self {
        Self { state, log_control }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) enum ProfilesStatus {
    /// Legacy entry points have no profile source attached.
    #[default]
    Disconnected,
    Loading,
    Ready(Arc<Vec<ProfileSummary>>),
    Error,
}

#[derive(Clone)]
pub(super) struct ProfilesFeed {
    updates: watch::Receiver<ProfilesStatus>,
    refresh: mpsc::Sender<()>,
}

impl ProfilesFeed {
    pub(super) fn snapshot(&self) -> ProfilesStatus {
        if self.updates.has_changed().is_err() {
            ProfilesStatus::Error
        } else {
            self.updates.borrow().clone()
        }
    }

    pub(super) fn request_refresh(&self) {
        match self.refresh.try_send(()) {
            Ok(()) | Err(mpsc::error::TrySendError::Full(())) => {
                // One queued request is enough: the worker always reads current committed data.
            }
            Err(mpsc::error::TrySendError::Closed(())) => {
                tracing::warn!("Terminal profile overview refresh worker is unavailable");
            }
        }
    }
}

pub(super) struct ProfileMonitor {
    pub(super) feed: ProfilesFeed,
    task: Option<JoinHandle<()>>,
}

impl ProfileMonitor {
    pub(super) fn start(state: AppState) -> Self {
        let (updates, receiver) = watch::channel(ProfilesStatus::Loading);
        let (refresh, requests) = mpsc::channel(1);
        let task = tokio::spawn(refresh_profiles(state, updates, requests, REFRESH_INTERVAL));
        Self {
            feed: ProfilesFeed {
                updates: receiver,
                refresh,
            },
            task: Some(task),
        }
    }

    pub(super) async fn stop(&mut self) -> io::Result<()> {
        if let Some(task) = self.task.take() {
            // This worker only reads. Cancellation cannot interrupt a profile write or commit.
            task.abort();
            match task.await {
                Ok(()) => {}
                Err(error) if error.is_cancelled() => {}
                Err(error) => {
                    return Err(io::Error::other(format!(
                        "profile overview task failed: {error}"
                    )));
                }
            }
        }
        Ok(())
    }
}

impl Drop for ProfileMonitor {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            // Also stop the reader if the owning terminal future is cancelled or panics.
            task.abort();
        }
    }
}

async fn refresh_profiles(
    state: AppState,
    updates: watch::Sender<ProfilesStatus>,
    mut requests: mpsc::Receiver<()>,
    interval: Duration,
) {
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticks.tick() => {},
            request = requests.recv() => if request.is_none() { break; },
            _ = updates.closed() => break,
        }
        let video_ids = {
            let index = state.index_cache.read().await;
            index
                .iter()
                .map(|video| video.file_name.clone())
                .collect::<Vec<_>>()
        };
        let status = match state.profiles.library_metadata(&video_ids).await {
            Ok(metadata) => ProfilesStatus::Ready(Arc::new(metadata.profiles)),
            Err(error) => {
                tracing::warn!(error = %error, "Could not refresh terminal profile overview");
                ProfilesStatus::Error
            }
        };
        // Retain only the latest snapshot; a slow terminal never queues stale snapshots.
        updates.send_replace(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::video::{VideoFile, VideoFormat};
    use crate::profiles::ProfileManager;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::sync::{Mutex, RwLock};

    struct Fixture(PathBuf);

    impl Fixture {
        async fn state(&self) -> AppState {
            let profiles = Arc::new(ProfileManager::new(&self.0).await.expect("profile manager"));
            AppState {
                movie_directory: self.0.clone(),
                port: 8081,
                depth: 0,
                index_cache: Arc::new(RwLock::new(vec![VideoFile {
                    file_name: "movie.mp4".to_owned(),
                    display_name: "Movie".to_owned(),
                    file_size_mb: 1,
                    unix_timestamp: 0,
                    format: VideoFormat::Mp4,
                    thumbnail_path: None,
                }])),
                thumbnails: None,
                scan_lock: Arc::new(Mutex::new(())),
                profiles,
            }
        }

        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "droptube-tui-profiles-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).expect("test library");
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).expect("remove test library");
        }
    }

    async fn next_status(monitor: &mut ProfileMonitor) -> ProfilesStatus {
        monitor
            .feed
            .updates
            .changed()
            .await
            .expect("snapshot update");
        monitor.feed.snapshot()
    }

    #[tokio::test]
    async fn worker_initializes_without_creating_profiles_and_refreshes_committed_browser_edits() {
        let fixture = Fixture::new();
        let state = fixture.state().await;
        let mut monitor = ProfileMonitor::start(state.clone());
        assert!(matches!(monitor.feed.snapshot(), ProfilesStatus::Loading));
        let ProfilesStatus::Ready(profiles) = next_status(&mut monitor).await else {
            panic!("expected ready snapshot");
        };
        assert!(profiles.is_empty());
        assert!(!fixture.0.join(".droptube").exists());

        state
            .profiles
            .set_rating("alice".to_owned(), "movie.mp4".to_owned(), 4)
            .await
            .expect("browser rating");
        state
            .profiles
            .add_tag(
                "alice".to_owned(),
                "movie.mp4".to_owned(),
                "favorites".to_owned(),
            )
            .await
            .expect("browser tag");
        monitor.feed.request_refresh();
        let ProfilesStatus::Ready(profiles) = next_status(&mut monitor).await else {
            panic!("expected updated snapshot");
        };
        assert_eq!(
            profiles.as_ref(),
            &[ProfileSummary {
                name: "alice".to_owned(),
                rating_count: 1,
                tagged_video_count: 1,
                tag_count: 1
            }]
        );

        state.index_cache.write().await.clear();
        monitor.feed.request_refresh();
        let ProfilesStatus::Ready(profiles) = next_status(&mut monitor).await else {
            panic!("expected refreshed index");
        };
        assert_eq!(profiles[0].rating_count, 0);
        assert_eq!(profiles[0].tag_count, 0);
        monitor.stop().await.expect("stop monitor");
    }

    #[tokio::test]
    async fn corrupt_profile_snapshots_show_error_and_recover_after_repair() {
        let fixture = Fixture::new();
        let state = fixture.state().await;
        state
            .profiles
            .set_rating("alice".to_owned(), "movie.mp4".to_owned(), 4)
            .await
            .expect("initial rating");
        let metadata = fixture.0.join(".droptube/profiles/alice/ratings.json");
        tokio::fs::write(&metadata, b"corrupt")
            .await
            .expect("corrupt fixture");
        let mut monitor = ProfileMonitor::start(state);
        assert!(matches!(
            next_status(&mut monitor).await,
            ProfilesStatus::Error
        ));
        tokio::fs::write(&metadata, br#"{"movie.mp4":4.5}"#)
            .await
            .expect("repair fixture");
        monitor.feed.request_refresh();
        assert!(matches!(
            next_status(&mut monitor).await,
            ProfilesStatus::Ready(_)
        ));
        monitor.stop().await.expect("stop monitor");
    }

    #[tokio::test]
    async fn queued_refreshes_are_bounded_and_shutdown_joins_a_blocked_reader() {
        let fixture = Fixture::new();
        let state = fixture.state().await;
        let _index_guard = state.index_cache.write().await;
        let mut monitor = ProfileMonitor::start(state.clone());
        monitor.feed.request_refresh();
        monitor.feed.request_refresh();
        assert_eq!(monitor.feed.refresh.capacity(), 0);
        assert!(matches!(monitor.feed.snapshot(), ProfilesStatus::Loading));
        monitor.stop().await.expect("cancel blocked reader");
        assert!(monitor.task.is_none());
        assert!(matches!(monitor.feed.snapshot(), ProfilesStatus::Error));
    }

    #[tokio::test]
    async fn background_refresh_discovers_split_profiles_without_writing() {
        let fixture = Fixture::new();
        let sources = [
            ("both/ratings.json", br#"{"movie.mp4":4.5}"#.as_slice()),
            ("both/tags.json", br#"{"movie.mp4":["shared"]}"#),
            (
                "ratingonly/ratings.json",
                br#"{"movie.mp4":0.5,"orphan.mp4":5}"#,
            ),
            ("tagonly/tags.json", br#"{"movie.mp4":["shared"]}"#),
            ("unrelated/notes.json", b"not profile contributions"),
            ("pending/ratings.json.1.tmp", b"incomplete"),
        ];
        let root = fixture.0.join(".droptube/profiles");
        for (relative, bytes) in sources {
            let path = root.join(relative);
            tokio::fs::create_dir_all(path.parent().expect("profile parent"))
                .await
                .expect("profile directory");
            tokio::fs::write(path, bytes).await.expect("source fixture");
        }
        let state = fixture.state().await;
        let mut monitor = ProfileMonitor::start(state);
        for refresh in 0..2 {
            if refresh != 0 {
                monitor.feed.request_refresh();
            }
            let ProfilesStatus::Ready(profiles) = next_status(&mut monitor).await else {
                panic!("expected committed profiles");
            };
            assert_eq!(
                profiles
                    .iter()
                    .map(|profile| profile.name.as_str())
                    .collect::<Vec<_>>(),
                ["both", "ratingonly", "tagonly"]
            );
            assert_eq!(profiles[1].rating_count, 1);
            assert_eq!(profiles[1].tag_count, 0);
            assert_eq!(profiles[2].rating_count, 0);
            assert_eq!(profiles[2].tag_count, 1);
        }
        monitor.stop().await.expect("stop reader");
        for (relative, bytes) in sources {
            assert_eq!(
                tokio::fs::read(root.join(relative))
                    .await
                    .expect("unchanged fixture"),
                bytes
            );
        }
        assert!(!root.join("ratingonly/tags.json").exists());
        assert!(!root.join("tagonly/ratings.json").exists());
    }

    #[tokio::test]
    async fn dropping_monitor_aborts_the_worker_and_closes_its_feed() {
        let fixture = Fixture::new();
        let state = fixture.state().await;
        let _index_guard = state.index_cache.write().await;
        let monitor = ProfileMonitor::start(state.clone());
        let mut feed = monitor.feed.clone();
        drop(monitor);
        assert!(feed.updates.changed().await.is_err());
        assert!(matches!(feed.snapshot(), ProfilesStatus::Error));
    }

    #[tokio::test]
    async fn context_retains_the_servers_shared_profile_manager_and_index() {
        let fixture = Fixture::new();
        let state = fixture.state().await;
        let context = ProfileTuiContext::new(state.clone(), TuiLogControl::new());
        assert!(Arc::ptr_eq(&context.state.profiles, &state.profiles));
        assert!(Arc::ptr_eq(&context.state.index_cache, &state.index_cache));
    }
}
