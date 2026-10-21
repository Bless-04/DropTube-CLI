//! Split-file storage and exact half-star regression tests.

use super::tests::Library;
use super::*;

#[tokio::test]
async fn unrelated_files_never_supply_contributions_or_trigger_imports() {
    let library = Library::new().await;
    let directory = library
        .committed("alice")
        .parent()
        .expect("profile directory")
        .to_path_buf();
    fs::create_dir_all(&directory)
        .await
        .expect("profile directory");
    let unrelated = directory.join("metadata.json");
    let bytes = br#"{"ratings":{"film.mp4":5},"tags":{"film.mp4":["ignored"]}}"#;
    fs::write(&unrelated, bytes).await.expect("unrelated file");
    let manager = library.manager().await;
    assert_eq!(
        manager.read("alice").await.expect("missing domains"),
        UserMetadata::default()
    );
    assert!(
        manager
            .library_metadata(&["film.mp4".into()])
            .await
            .expect("no saved contributions")
            .profiles
            .is_empty()
    );
    assert!(!library.committed("alice").exists());
    manager
        .set_rating("alice".into(), "new.mp4".into(), 4.5)
        .await
        .expect("ordinary first rating");
    let metadata = manager.read("alice").await.expect("saved contributions");
    assert_eq!(metadata.ratings.len(), 1);
    assert_eq!(metadata.ratings["new.mp4"].stars(), 4.5);
    assert!(metadata.tags.is_empty());
    assert!(!unrelated.with_file_name("tags.json").exists());
    assert_eq!(
        fs::read(&unrelated)
            .await
            .expect("unrelated bytes unchanged"),
        bytes
    );
    let mut entries = fs::read_dir(&directory).await.expect("profile files");
    let mut names = Vec::new();
    while let Some(entry) = entries.next_entry().await.expect("entry") {
        names.push(entry.file_name());
    }
    names.sort();
    assert_eq!(
        names,
        [
            std::ffi::OsString::from("metadata.json"),
            std::ffi::OsString::from("ratings.json")
        ]
    );
}

#[tokio::test]
async fn wrong_domain_file_types_reject_reads_and_edits() {
    for domain in [Domain::Ratings, Domain::Tags] {
        let library = Library::new().await;
        let path = library.committed("alice").with_file_name(domain.filename());
        fs::create_dir_all(&path)
            .await
            .expect("wrong file type fixture");
        let manager = library.manager().await;
        assert!(matches!(
            manager.read("alice").await,
            Err(ProfileError::UnsafePath(_))
        ));
        assert!(matches!(
            manager
                .set_rating("alice".into(), "film.mp4".into(), 4.5)
                .await,
            Err(ProfileError::UnsafePath(_))
        ));
        assert!(matches!(
            manager.library_metadata(&["film.mp4".into()]).await,
            Err(ProfileError::UnsafePath(_))
        ));
        assert!(
            fs::metadata(&path)
                .await
                .expect("preserved fixture")
                .is_dir()
        );
    }
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
