//! Split storage, migration recovery, and exact half-star regression tests.

use super::tests::Library;
use super::*;

#[cfg(windows)]
#[tokio::test]
async fn denied_legacy_archive_keeps_both_copies_and_original_without_applying_the_edit() {
    use std::os::windows::fs::OpenOptionsExt;
    let library = Library::new().await;
    let source = br#"{"ratings":{"film.mp4":4},"tags":{"film.mp4":["old"]}}"#;
    library.write_metadata("alice", source).await;
    let legacy = library.committed("alice").with_file_name("metadata.json");
    // Readers and writers may open the legacy file, but rename/delete is denied.
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&legacy)
        .expect("hold legacy without delete sharing");
    let manager = library.manager().await;
    assert!(matches!(
        manager
            .set_rating("alice".into(), "film.mp4".into(), 0.5)
            .await,
        Err(ProfileError::Persistence {
            operation: "archive legacy profile metadata",
            ..
        })
    ));
    assert_eq!(fs::read(&legacy).await.expect("preserved source"), source);
    assert!(legacy.with_file_name("ratings.json").exists());
    assert!(legacy.with_file_name("tags.json").exists());
    assert_eq!(
        manager
            .read("alice")
            .await
            .expect("committed unchanged copies")
            .ratings["film.mp4"]
            .stars(),
        4.0
    );
    let mut entries = fs::read_dir(legacy.parent().expect("profile directory"))
        .await
        .expect("entries");
    let mut count = 0;
    while entries.next_entry().await.expect("entry").is_some() {
        count += 1;
    }
    assert_eq!(count, 3, "failed archive reservation is cleaned up");
    drop(held);
    manager
        .set_rating("alice".into(), "film.mp4".into(), 0.5)
        .await
        .expect("retry after archive denial");
    assert!(!legacy.exists());
    assert_eq!(
        manager
            .read("alice")
            .await
            .expect("edited split data")
            .ratings["film.mp4"]
            .stars(),
        0.5
    );
}

#[test]
fn exact_half_star_type_round_trips_stars_and_rejects_every_invalid_kind() {
    for units in 1..=10 {
        let stars = f64::from(units) / 2.0;
        let rating = PersonalRating::new(stars).expect("valid half step");
        assert_eq!(rating.stars(), stars);
        let json = serde_json::to_string(&rating).expect("serialize stars");
        assert_eq!(
            serde_json::from_str::<PersonalRating>(&json).expect("round trip"),
            rating
        );
        assert_eq!(
            serde_json::from_str::<f64>(&json).expect("numeric stars"),
            stars
        );
    }
    assert_eq!(
        serde_json::to_string(&PersonalRating::new(4.5).expect("rating")).expect("JSON"),
        "4.5"
    );
    for stars in [
        0.0,
        -1.0,
        0.25,
        4.2,
        5.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert!(matches!(
            PersonalRating::new(stars),
            Err(ProfileError::InvalidRating)
        ));
    }
    for json in [
        "0", "-1", "0.25", "4.2", "5.5", "null", "true", "\"4.5\"", "[]", "{}", "NaN", "1e999",
    ] {
        assert!(
            serde_json::from_str::<PersonalRating>(json).is_err(),
            "{json}"
        );
    }
}

#[tokio::test]
async fn split_edits_create_only_their_domain_and_never_rewrite_the_other_file() {
    let library = Library::new().await;
    let manager = library.manager().await;
    manager
        .set_rating("alice".into(), "film.mp4".into(), 4.5)
        .await
        .expect("rating edit");
    let ratings = library.committed("alice");
    let tags = ratings.with_file_name("tags.json");
    assert!(!tags.exists());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(&ratings).await.expect("rating bytes")
        )
        .expect("JSON"),
        serde_json::json!({"film.mp4":4.5})
    );
    let timestamp = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    std::fs::File::options()
        .write(true)
        .open(&ratings)
        .expect("ratings handle")
        .set_times(std::fs::FileTimes::new().set_modified(timestamp))
        .expect("fixed timestamp");
    let original = fs::read(&ratings).await.expect("prior rating bytes");
    manager
        .add_tag("alice".into(), "film.mp4".into(), "Favorites".into())
        .await
        .expect("tag edit");
    assert_eq!(fs::read(&ratings).await.expect("rating bytes"), original);
    assert_eq!(
        fs::metadata(&ratings)
            .await
            .expect("rating metadata")
            .modified()
            .expect("mtime"),
        timestamp
    );
    let tag_bytes = fs::read(&tags).await.expect("tag bytes");
    manager
        .set_rating("alice".into(), "film.mp4".into(), 0.5)
        .await
        .expect("rerating");
    assert_eq!(fs::read(&tags).await.expect("unchanged tags"), tag_bytes);
    manager
        .remove_tag("tagonly".into(), "film.mp4".into(), "absent".into())
        .await
        .expect("empty tag edit");
    assert!(!library.committed("tagonly").exists());
    assert_eq!(
        fs::read(library.committed("tagonly").with_file_name("tags.json"))
            .await
            .expect("empty tags"),
        b"{}"
    );
}

#[tokio::test]
async fn legacy_reads_are_read_only_and_first_edit_archives_exact_bytes_including_orphans() {
    for source in [
        b"{}".as_slice(),
        br#"{"ratings":{"film.mp4":4,"orphan.mkv":2}}"#,
        br#"{"tags":{"film.mp4":["favorites"],"orphan.mkv":["keep"]}}"#,
        br#"{ "ratings": {"film.mp4":4}, "tags":{"film.mp4":["favorites"]} }"#,
    ] {
        let library = Library::new().await;
        library.write_metadata("alice", source).await;
        let manager = library.manager().await;
        let legacy = library.committed("alice").with_file_name("metadata.json");
        let prior = manager.read("alice").await.expect("legacy read");
        manager
            .library_metadata(&["film.mp4".into()])
            .await
            .expect("read-only discovery");
        assert!(!library.committed("alice").exists());
        assert_eq!(fs::read(&legacy).await.expect("unchanged legacy"), source);
        let backup = legacy.with_file_name("metadata.legacy.prior.json");
        fs::write(&backup, b"earlier backup")
            .await
            .expect("old backup");
        manager
            .add_tag("alice".into(), "new.mp4".into(), "new".into())
            .await
            .expect("migrate then edit");
        assert!(!legacy.exists());
        assert_eq!(
            fs::read(&backup).await.expect("old backup preserved"),
            b"earlier backup"
        );
        let current = manager.read("alice").await.expect("canonical read");
        assert_eq!(current.ratings, prior.ratings);
        for (id, labels) in prior.tags {
            assert_eq!(current.tags[&id], labels);
        }
        let mut entries = fs::read_dir(legacy.parent().expect("profile dir"))
            .await
            .expect("entries");
        let mut archived = 0;
        while let Some(entry) = entries.next_entry().await.expect("entry") {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("metadata.legacy.") && entry.path() != backup {
                assert_eq!(fs::read(entry.path()).await.expect("backup bytes"), source);
                archived += 1;
            }
        }
        assert_eq!(archived, 1);
    }
}

#[tokio::test]
async fn canonical_domains_override_legacy_even_when_empty_and_backups_never_resurrect_data() {
    let library = Library::new().await;
    library
        .write_metadata(
            "alice",
            br#"{"ratings":{"film.mp4":1},"tags":{"film.mp4":["old"]}}"#,
        )
        .await;
    let ratings = library.committed("alice");
    fs::write(&ratings, b"{}")
        .await
        .expect("authoritative empty ratings");
    let manager = library.manager().await;
    let before = manager.read("alice").await.expect("mixed read");
    assert!(before.ratings.is_empty());
    assert!(before.tags["film.mp4"].contains("old"));
    manager
        .set_rating("alice".into(), "other.mp4".into(), 4.5)
        .await
        .expect("resume migration");
    assert!(
        !manager
            .read("alice")
            .await
            .expect("read")
            .ratings
            .contains_key("film.mp4")
    );
    fs::remove_file(ratings.with_file_name("tags.json"))
        .await
        .expect("missing retired domain");
    assert!(
        library
            .manager()
            .await
            .read("alice")
            .await
            .expect("restart read")
            .tags
            .is_empty()
    );
    fs::remove_file(&ratings)
        .await
        .expect("backup-only profile");
    assert!(
        manager
            .library_metadata(&[])
            .await
            .expect("backup-only discovery")
            .profiles
            .is_empty()
    );
}

#[tokio::test]
async fn interruption_at_each_migration_boundary_resumes_without_applying_the_failed_edit() {
    for step in 1..=4 {
        let library = Library::new().await;
        let source = br#"{"ratings":{"film.mp4":4,"orphan.mp4":2},"tags":{"film.mp4":["old"]}}"#;
        library.write_metadata("alice", source).await;
        let manager = library.manager().await;
        manager.fail_migration_step.store(step, Ordering::Relaxed);
        assert!(matches!(
            manager
                .set_rating("alice".into(), "film.mp4".into(), 0.5)
                .await,
            Err(ProfileError::Persistence { .. })
        ));
        let prior = manager
            .read("alice")
            .await
            .expect("partially migrated read");
        assert_eq!(prior.ratings["film.mp4"].stars(), 4.0);
        assert_eq!(prior.ratings["orphan.mp4"].stars(), 2.0);
        assert!(prior.tags["film.mp4"].contains("old"));
        // Simulate a newer committed canonical contribution before restart.
        fs::write(
            library.committed("alice"),
            br#"{"film.mp4":3.5,"orphan.mp4":2}"#,
        )
        .await
        .expect("newer canonical commit");
        let restarted = library.manager().await;
        restarted
            .add_tag("alice".into(), "film.mp4".into(), "new".into())
            .await
            .expect("restart and retry");
        let after = restarted.read("alice").await.expect("final read");
        assert_eq!(after.ratings["film.mp4"].stars(), 3.5);
        assert_eq!(
            after.tags["film.mp4"],
            HashSet::from(["old".into(), "new".into()])
        );
        assert!(
            !library
                .committed("alice")
                .with_file_name("metadata.json")
                .exists()
        );
        let mut entries = fs::read_dir(library.committed("alice").parent().expect("profile dir"))
            .await
            .expect("entries");
        while let Some(entry) = entries.next_entry().await.expect("entry") {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("metadata.legacy.")
            {
                assert_eq!(
                    fs::read(entry.path()).await.expect("recoverable backup"),
                    source
                );
            }
        }
    }
}

#[tokio::test]
async fn failed_migration_copy_and_failed_postmigration_edit_preserve_committed_data() {
    let library = Library::new().await;
    library
        .write_metadata(
            "alice",
            br#"{"ratings":{"film.mp4":4},"tags":{"film.mp4":["old"]}}"#,
        )
        .await;
    let manager = library.manager().await;
    manager.fail_before_replace.store(true, Ordering::Relaxed);
    assert!(
        manager
            .set_rating("alice".into(), "film.mp4".into(), 1.5)
            .await
            .is_err()
    );
    assert!(!library.committed("alice").exists());
    assert!(
        library
            .committed("alice")
            .with_file_name("metadata.json")
            .exists()
    );
    manager.fail_before_replace.store(false, Ordering::Relaxed);
    manager.fail_migration_step.store(4, Ordering::Relaxed);
    assert!(
        manager
            .set_rating("alice".into(), "film.mp4".into(), 1.5)
            .await
            .is_err()
    );
    assert!(
        !library
            .committed("alice")
            .with_file_name("metadata.json")
            .exists()
    );
    manager.fail_migration_step.store(0, Ordering::Relaxed);
    manager.fail_before_replace.store(true, Ordering::Relaxed);
    let before = manager.read("alice").await.expect("migrated metadata");
    assert!(
        manager
            .set_rating("alice".into(), "film.mp4".into(), 1.5)
            .await
            .is_err()
    );
    assert_eq!(
        manager.read("alice").await.expect("preserved state"),
        before
    );
    manager.fail_before_replace.store(false, Ordering::Relaxed);
    manager
        .set_rating("alice".into(), "film.mp4".into(), 1.5)
        .await
        .expect("retry edit");
    assert_eq!(
        library
            .manager()
            .await
            .read("alice")
            .await
            .expect("restart after edit")
            .ratings["film.mp4"]
            .stars(),
        1.5
    );
}

#[tokio::test]
async fn corrupt_canonical_files_never_fall_back_and_invalid_legacy_blocks_migration() {
    for filename in ["ratings.json", "tags.json"] {
        for bytes in [
            b"{broken".as_slice(),
            b"[]",
            b"null",
            br#"{"film.mp4":null}"#,
            br#"{"":[]}"#,
        ] {
            let library = Library::new().await;
            library
                .write_metadata("alice", br#"{"ratings":{"film.mp4":4},"tags":{}}"#)
                .await;
            let path = library.committed("alice").with_file_name(filename);
            fs::write(&path, bytes).await.expect("corrupt canonical");
            let manager = library.manager().await;
            assert!(manager.read("alice").await.is_err());
            assert!(
                manager
                    .set_rating("alice".into(), "film.mp4".into(), 3.5)
                    .await
                    .is_err()
            );
            assert!(
                manager
                    .library_metadata(&["film.mp4".into()])
                    .await
                    .is_err()
            );
            assert_eq!(fs::read(&path).await.expect("unchanged corruption"), bytes);
        }
        let library = Library::new().await;
        library.write_metadata("alice", b"corrupt legacy").await;
        let path = library.committed("alice").with_file_name(filename);
        fs::create_dir(&path).await.expect("wrong canonical type");
        assert!(matches!(
            library.manager().await.read("alice").await,
            Err(ProfileError::UnsafePath(_))
        ));
    }
    let library = Library::new().await;
    library.write_metadata("alice", b"corrupt legacy").await;
    fs::write(library.committed("alice"), b"{}")
        .await
        .expect("ratings");
    fs::write(
        library.committed("alice").with_file_name("tags.json"),
        b"{}",
    )
    .await
    .expect("tags");
    let manager = library.manager().await;
    assert_eq!(
        manager
            .read("alice")
            .await
            .expect("canonical reads need no legacy"),
        UserMetadata::default()
    );
    assert!(
        manager
            .set_rating("alice".into(), "film.mp4".into(), 3.5)
            .await
            .is_err()
    );
    assert_eq!(
        fs::read(library.committed("alice"))
            .await
            .expect("no edit applied"),
        b"{}"
    );
}

#[tokio::test]
async fn half_star_aggregates_keep_full_precision_and_discover_single_domain_profiles() {
    let library = Library::new().await;
    let manager = library.manager().await;
    manager
        .set_rating("p1".into(), "film.mp4".into(), 3.5)
        .await
        .expect("p1");
    manager
        .set_rating("p2".into(), "film.mp4".into(), 4.5)
        .await
        .expect("p2");
    manager
        .add_tag("tags".into(), "film.mp4".into(), "shared".into())
        .await
        .expect("tags only");
    let ids = ["film.mp4".into(), "minimum.mp4".into()];
    assert_eq!(
        manager.aggregate(&ids).await.expect("mean")["film.mp4"],
        AggregateRating {
            average: Some(4.0),
            count: 2
        }
    );
    manager
        .set_rating("p1".into(), "film.mp4".into(), 4)
        .await
        .expect("rerate");
    manager
        .set_rating("p2".into(), "minimum.mp4".into(), 0.5)
        .await
        .expect("minimum");
    let shared = library
        .manager()
        .await
        .library_metadata(&ids)
        .await
        .expect("restart");
    assert_eq!(
        shared.ratings["film.mp4"],
        AggregateRating {
            average: Some(4.25),
            count: 2
        }
    );
    assert_eq!(
        shared.ratings["minimum.mp4"],
        AggregateRating {
            average: Some(0.5),
            count: 1
        }
    );
    assert_eq!(shared.profiles.len(), 3);
    assert_eq!(shared.profiles[0].rating_count, 1);
    assert_eq!(shared.profiles[0].tag_count, 0);
    assert_eq!(shared.profiles[2].rating_count, 0);
    assert_eq!(shared.profiles[2].tag_count, 1);
}
