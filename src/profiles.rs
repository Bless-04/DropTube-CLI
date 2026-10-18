//! Persistent profile contributions for one canonical served media library.
//!
//! Profiles are shared names rather than authenticated identities. Transactions
//! own a per-profile lock until the committed file is replaced, including when a
//! request stops awaiting its result. Multiple writing server processes and
//! hostile local filesystem replacement races are not supported.

use crate::utils::prepare_droptube_directory;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// One profile's integer ratings and additive, normalized tag sets.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct UserMetadata {
    /// Each library-relative video identifier has at most one personal rating.
    pub ratings: HashMap<String, u8>,
    /// Tags belong to this profile, never to the shared media index.
    pub tags: HashMap<String, HashSet<String>>,
}

/// A derived public community rating; no individual contributions are exposed.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AggregateRating {
    /// The unrounded arithmetic mean, or `None` when there are no contributions.
    pub average: Option<f64>,
    /// Number of profiles with a rating, excluding unrated profiles.
    pub count: u64,
}

/// Public contributions derived from committed profiles for the current index.
///
/// Personal metadata remains separate: these tags are a union, and each rating
/// is an average rather than an individual profile's contribution.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibraryMetadata {
    /// Community rating for every requested video, including unrated videos.
    pub ratings: HashMap<String, AggregateRating>,
    /// Union of canonical labels across profiles, omitting untagged videos.
    pub tags: HashMap<String, HashSet<String>>,
    /// Committed profiles in normalized-name order, with indexed counts only.
    pub profiles: Vec<ProfileSummary>,
}

/// Read-only contribution counts for one committed, shared profile identity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileSummary {
    /// Validated lowercase profile name used by the metadata API and storage.
    pub name: String,
    /// Number of currently indexed videos rated by this profile.
    pub rating_count: usize,
    /// Number of currently indexed videos with at least one profile tag.
    pub tagged_video_count: usize,
    /// Number of distinct canonical labels used on currently indexed videos.
    pub tag_count: usize,
}

/// Profile input, persistence, corruption, and transaction failures.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    /// An identifier failed the complete profile-name whitelist.
    #[error("invalid profile name")]
    InvalidProfile,
    /// Personal ratings must be integers between one and five.
    #[error("rating must be an integer from 1 through 5")]
    InvalidRating,
    /// Tag input failed normalization or the Unicode character limit.
    #[error("tag must be a nonempty label of at most 64 characters without controls")]
    InvalidTag,
    /// A filesystem operation failed; its path is intended only for server logs.
    #[error("could not {operation} at {path}: {source}")]
    Persistence {
        /// Operation attempted when the error occurred.
        operation: &'static str,
        /// Internal path; HTTP responses must not expose this value.
        path: PathBuf,
        /// Original filesystem error.
        #[source]
        source: io::Error,
    },
    /// A committed file cannot be interpreted as valid profile contributions.
    #[error("committed profile metadata at {path} is invalid: {detail}")]
    Corrupt {
        /// Internal committed path, intended only for server logs.
        path: PathBuf,
        /// Diagnostic reason, intended only for server logs.
        detail: String,
    },
    /// A storage component is a link, wrong file type, or outside the library.
    #[error("profile storage has an unsafe path component: {0}")]
    UnsafePath(PathBuf),
    /// An owned transaction task could not complete normally.
    #[error("profile task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
    /// Aggregate counters exceeded their representable range.
    #[error("community rating counters exceeded their supported range")]
    AggregateOverflow,
}

/// Validate a complete ASCII alphanumeric profile identifier and lowercase it.
///
/// Names are not trimmed, and Windows device names are never accepted.
///
/// ```
/// use droptube::profiles::sanitize_profile_name;
/// assert_eq!(sanitize_profile_name("Alice2"), Some("alice2".to_owned()));
/// assert_eq!(sanitize_profile_name(" alice"), None);
/// assert_eq!(sanitize_profile_name("COM1"), None);
/// ```
pub fn sanitize_profile_name(input: &str) -> Option<String> {
    if input.is_empty()
        || input.len() > 32
        || !input.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    let normalized = input.to_ascii_lowercase();
    if matches!(normalized.as_str(), "con" | "prn" | "aux" | "nul")
        || matches!(
            normalized.as_bytes(),
            [b'c', b'o', b'm', b'1'..=b'9'] | [b'l', b'p', b't', b'1'..=b'9']
        )
    {
        None
    } else {
        Some(normalized)
    }
}

/// Trim and lowercase a free-form tag, rejecting controls and overlong labels.
///
/// The limit counts Unicode scalar values after normalization, including any
/// characters introduced by lowercase conversion. Controls are rejected even
/// when surrounding whitespace would otherwise be trimmed.
///
/// ```
/// use droptube::profiles::normalize_tag;
/// assert_eq!(normalize_tag("  Sci-Fi  "), Some("sci-fi".to_owned()));
/// assert_eq!(normalize_tag("\tfilm"), None);
/// ```
pub fn normalize_tag(input: &str) -> Option<String> {
    if input.chars().any(char::is_control) {
        return None;
    }
    let normalized = input.trim().to_lowercase();
    if normalized.is_empty() || normalized.chars().count() > 64 {
        None
    } else {
        Some(normalized)
    }
}

/// Asynchronous storage and shared transaction locks for one media library.
pub struct ProfileManager {
    library_root: PathBuf,
    profiles_root: PathBuf,
    cookie_name: String,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    next_temporary: AtomicU64,
    #[cfg(test)]
    fail_before_replace: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    transaction_finished: tokio::sync::Notify,
}

enum Mutation {
    Rating(u8),
    AddTag(String),
    RemoveTag(String),
}

impl ProfileManager {
    /// Bind storage to a canonical existing library without creating files.
    ///
    /// Cookie names use documented FNV-1a 64-bit hashing of canonical native path
    /// bytes: a platform discriminator followed by Unix bytes or Windows UTF-16
    /// little-endian units. The fixed hash is stable across restarts and ports.
    pub async fn new(root: &Path) -> Result<Self, ProfileError> {
        let library_root = fs::canonicalize(root)
            .await
            .map_err(|source| persistence("canonicalize library", root, source))?;
        let metadata = fs::metadata(&library_root)
            .await
            .map_err(|source| persistence("inspect library", &library_root, source))?;
        if !metadata.is_dir() {
            return Err(ProfileError::UnsafePath(library_root));
        }
        let cookie_name = format!("droptube_profile_{:016x}", library_hash(&library_root));
        Ok(Self {
            profiles_root: library_root.join(".droptube").join("profiles"),
            library_root,
            cookie_name,
            locks: Mutex::new(HashMap::new()),
            next_temporary: AtomicU64::new(0),
            #[cfg(test)]
            fail_before_replace: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            transaction_finished: tokio::sync::Notify::new(),
        })
    }

    /// The host-only cookie name belonging to this canonical library.
    pub fn cookie_name(&self) -> &str {
        &self.cookie_name
    }

    /// Canonical served-library root used for every profile persistence path.
    pub fn library_root(&self) -> &Path {
        &self.library_root
    }

    /// Read committed contributions, returning defaults only for missing storage.
    ///
    /// This never creates a profile directory or committed file.
    pub async fn read(&self, profile: &str) -> Result<UserMetadata, ProfileError> {
        let profile = validate_profile(profile)?;
        let lock = self.profile_lock(&profile).await;
        let _guard = lock.lock().await;
        self.load(&profile).await
    }

    /// Assign one personal rating, replacing the previous contribution.
    ///
    /// Video IDs are dictionary keys; handlers must first verify index membership.
    /// Dropping the returned future after the task starts does not cancel its save.
    pub async fn set_rating(
        self: &Arc<Self>,
        profile: String,
        video_id: String,
        rating: u8,
    ) -> Result<UserMetadata, ProfileError> {
        let profile = validate_profile(&profile)?;
        if !(1..=5).contains(&rating) {
            return Err(ProfileError::InvalidRating);
        }
        self.mutate(profile, video_id, Mutation::Rating(rating))
            .await
    }

    /// Add one normalized label without replacing any existing tags.
    pub async fn add_tag(
        self: &Arc<Self>,
        profile: String,
        video_id: String,
        tag: String,
    ) -> Result<UserMetadata, ProfileError> {
        let profile = validate_profile(&profile)?;
        let tag = normalize_tag(&tag).ok_or(ProfileError::InvalidTag)?;
        self.mutate(profile, video_id, Mutation::AddTag(tag)).await
    }

    /// Remove only one normalized label; removing an absent label is idempotent.
    pub async fn remove_tag(
        self: &Arc<Self>,
        profile: String,
        video_id: String,
        tag: String,
    ) -> Result<UserMetadata, ProfileError> {
        let profile = validate_profile(&profile)?;
        let tag = normalize_tag(&tag).ok_or(ProfileError::InvalidTag)?;
        self.mutate(profile, video_id, Mutation::RemoveTag(tag))
            .await
    }

    /// Derive public averages once from committed normalized profile directories.
    ///
    /// Missing files contribute nothing. Corruption or unexpected I/O aborts the
    /// whole result, so callers cannot display a misleading partial average.
    /// Unrelated directory names are not profile identities. Windows casing
    /// aliases resolve through the same normalized identity used by reads.
    pub async fn aggregate(
        &self,
        video_ids: &[String],
    ) -> Result<HashMap<String, AggregateRating>, ProfileError> {
        Ok(self.library_metadata(video_ids).await?.ratings)
    }

    /// Derive shared ratings, tag unions, and read-only profile summaries.
    ///
    /// Only indexed video IDs contribute to results or counts. Missing committed
    /// files contribute nothing and are never created by this read. Corruption
    /// or unexpected I/O fails the whole view rather than returning partial data.
    /// Each profile is read under its transaction lock; the view is not a global
    /// snapshot across concurrently edited profiles. Cancelling this read is safe
    /// because it performs no writes.
    ///
    /// ```no_run
    /// # async fn example(root: &std::path::Path) -> Result<(), droptube::profiles::ProfileError> {
    /// let profiles = droptube::profiles::ProfileManager::new(root).await?;
    /// let metadata = profiles.library_metadata(&["film.mp4".to_owned()]).await?;
    /// assert!(metadata.ratings.contains_key("film.mp4"));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn library_metadata(
        &self,
        video_ids: &[String],
    ) -> Result<LibraryMetadata, ProfileError> {
        let mut totals: HashMap<String, (u64, u64)> =
            video_ids.iter().map(|id| (id.clone(), (0, 0))).collect();
        let mut tags: HashMap<String, HashSet<String>> = HashMap::new();
        let mut profiles = Vec::new();
        if self.inspect_storage(None).await?.is_none() {
            return Ok(LibraryMetadata {
                ratings: finish_aggregate(totals),
                tags,
                profiles,
            });
        }
        let mut entries = fs::read_dir(&self.profiles_root)
            .await
            .map_err(|source| persistence("list profiles", &self.profiles_root, source))?;
        let mut seen_profiles = HashSet::new();
        while let Some(entry) = entries.next_entry().await.map_err(|source| {
            persistence("read profile directory entry", &self.profiles_root, source)
        })? {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(profile) = sanitize_profile_name(name)
                .filter(|normalized| cfg!(windows) || normalized == name)
            else {
                continue;
            };
            // Windows can resolve differently cased existing directories through
            // the normalized API path. Count that identity once, matching reads.
            if !seen_profiles.insert(profile.clone()) {
                continue;
            }
            let lock = self.profile_lock(&profile).await;
            let _guard = lock.lock().await;
            let Some(metadata) = self.load_committed(&profile).await? else {
                continue;
            };
            let rating_count = metadata
                .ratings
                .keys()
                .filter(|id| totals.contains_key(*id))
                .count();
            let tagged_video_count = metadata
                .tags
                .iter()
                .filter(|(id, labels)| totals.contains_key(*id) && !labels.is_empty())
                .count();
            let tag_count = metadata
                .tags
                .iter()
                .filter(|(id, _)| totals.contains_key(*id))
                .flat_map(|(_, labels)| labels)
                .collect::<HashSet<_>>()
                .len();
            for (video_id, rating) in metadata.ratings {
                if let Some(total) = totals.get_mut(&video_id) {
                    accumulate_rating(total, rating)?;
                }
            }
            for (video_id, labels) in metadata.tags {
                if totals.contains_key(&video_id) && !labels.is_empty() {
                    tags.entry(video_id).or_default().extend(labels);
                }
            }
            profiles.push(ProfileSummary {
                name: profile,
                rating_count,
                tagged_video_count,
                tag_count,
            });
        }
        profiles.sort_unstable_by(|first, second| first.name.cmp(&second.name));
        Ok(LibraryMetadata {
            ratings: finish_aggregate(totals),
            tags,
            profiles,
        })
    }

    async fn profile_lock(&self, profile: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        Arc::clone(
            locks
                .entry(profile.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    async fn mutate(
        self: &Arc<Self>,
        profile: String,
        video_id: String,
        mutation: Mutation,
    ) -> Result<UserMetadata, ProfileError> {
        let manager = Arc::clone(self);
        // The owned task keeps the lock until even Tokio's filesystem work has
        // completed. Cancelling its caller cannot release a transaction mid-save.
        tokio::spawn(async move {
            let lock = manager.profile_lock(&profile).await;
            let _guard = lock.lock().await;
            let result = async {
                let mut metadata = manager.load(&profile).await?;
                match mutation {
                    Mutation::Rating(rating) => {
                        metadata.ratings.insert(video_id, rating);
                    }
                    Mutation::AddTag(tag) => {
                        metadata.tags.entry(video_id).or_default().insert(tag);
                    }
                    Mutation::RemoveTag(tag) => {
                        if let Some(tags) = metadata.tags.get_mut(&video_id) {
                            tags.remove(&tag);
                            if tags.is_empty() {
                                metadata.tags.remove(&video_id);
                            }
                        }
                    }
                }
                manager.persist(&profile, metadata).await
            }
            .await;
            if let Err(error) = &result {
                tracing::error!(%error, "Profile transaction failed");
            }
            #[cfg(test)]
            manager.transaction_finished.notify_one();
            result
        })
        .await?
    }

    async fn load(&self, profile: &str) -> Result<UserMetadata, ProfileError> {
        Ok(self.load_committed(profile).await?.unwrap_or_default())
    }

    async fn load_committed(&self, profile: &str) -> Result<Option<UserMetadata>, ProfileError> {
        if self.inspect_storage(Some(profile)).await?.is_none() {
            return Ok(None);
        }
        let path = self.profiles_root.join(profile).join("metadata.json");
        let bytes = match fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                // Only a missing committed file is an empty profile. Recheck
                // parents so a removed library or changed link is not concealed.
                self.inspect_storage(Some(profile)).await?;
                return Ok(None);
            }
            Err(source) => {
                return Err(persistence(
                    "read committed profile metadata",
                    &path,
                    source,
                ));
            }
        };
        tokio::task::spawn_blocking(move || decode_metadata(&bytes, &path).map(Some)).await?
    }

    async fn persist(
        &self,
        profile: &str,
        metadata: UserMetadata,
    ) -> Result<UserMetadata, ProfileError> {
        self.inspect_storage(Some(profile)).await?;
        prepare_droptube_directory(&self.library_root)
            .await
            .map_err(|source| {
                persistence(
                    "prepare DropTube directory",
                    &self.library_root.join(".droptube"),
                    source,
                )
            })?;
        self.inspect_storage(Some(profile)).await?;
        let directory = self.profiles_root.join(profile);
        fs::create_dir_all(&directory)
            .await
            .map_err(|source| persistence("create profile directory", &directory, source))?;
        self.inspect_storage(Some(profile)).await?;
        let committed = directory.join("metadata.json");
        let serialization_path = committed.clone();
        // Move rather than clone the contribution maps while serialization runs
        // off the async runtime, then return the same state after its commit.
        let (metadata, bytes) = tokio::task::spawn_blocking(move || {
            serde_json::to_vec_pretty(&metadata)
                .map(|bytes| (metadata, bytes))
                .map_err(|error| ProfileError::Corrupt {
                    path: serialization_path,
                    detail: error.to_string(),
                })
        })
        .await??;
        let (temporary, mut file) = create_temporary(&directory, &self.next_temporary).await?;
        let result = async {
            file.write_all(&bytes).await.map_err(|source| {
                persistence("write temporary profile metadata", &temporary, source)
            })?;
            file.sync_all().await.map_err(|source| {
                persistence("sync temporary profile metadata", &temporary, source)
            })?;
            drop(file);
            self.inspect_storage(Some(profile)).await?;
            #[cfg(test)]
            if self.fail_before_replace.load(Ordering::Relaxed) {
                return Err(persistence(
                    "replace committed profile metadata",
                    &committed,
                    io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "injected replacement failure",
                    ),
                ));
            }
            fs::rename(&temporary, &committed).await.map_err(|source| {
                persistence("replace committed profile metadata", &committed, source)
            })
        }
        .await;
        if result.is_err()
            && let Err(error) = fs::remove_file(&temporary).await
        {
            tracing::warn!(path = %temporary.display(), %error, "Could not clean up failed profile temporary file");
        }
        result.map(|()| metadata)
    }

    // The optional metadata means the requested storage component is missing.
    // Every existing ancestor is still checked before missing data is accepted.
    async fn inspect_storage(
        &self,
        profile: Option<&str>,
    ) -> Result<Option<Metadata>, ProfileError> {
        self.inspect_component(&self.library_root, true)
            .await?
            .ok_or_else(|| {
                persistence(
                    "inspect library",
                    &self.library_root,
                    io::Error::new(io::ErrorKind::NotFound, "library is missing"),
                )
            })?;
        if self
            .inspect_component(&self.library_root.join(".droptube"), true)
            .await?
            .is_none()
        {
            return Ok(None);
        }
        let profiles = self.inspect_component(&self.profiles_root, true).await?;
        if profiles.is_none() {
            return Ok(None);
        }
        let Some(profile) = profile else {
            return Ok(profiles);
        };
        let directory = self.profiles_root.join(profile);
        if self.inspect_component(&directory, true).await?.is_none() {
            return Ok(None);
        }
        self.inspect_component(&directory.join("metadata.json"), false)
            .await
    }

    async fn inspect_component(
        &self,
        path: &Path,
        directory: bool,
    ) -> Result<Option<Metadata>, ProfileError> {
        let metadata = match fs::symlink_metadata(path).await {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(persistence("inspect profile storage", path, source)),
        };
        if is_link(&metadata)
            || (directory && !metadata.is_dir())
            || (!directory && !metadata.is_file())
        {
            return Err(ProfileError::UnsafePath(path.to_path_buf()));
        }
        let canonical = fs::canonicalize(path)
            .await
            .map_err(|source| persistence("resolve profile storage", path, source))?;
        if !canonical.starts_with(&self.library_root) {
            return Err(ProfileError::UnsafePath(path.to_path_buf()));
        }
        Ok(Some(metadata))
    }
}

fn validate_profile(profile: &str) -> Result<String, ProfileError> {
    sanitize_profile_name(profile).ok_or(ProfileError::InvalidProfile)
}

fn decode_metadata(bytes: &[u8], path: &Path) -> Result<UserMetadata, ProfileError> {
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return Err(ProfileError::Corrupt {
            path: path.to_path_buf(),
            detail: "stored metadata must be a JSON object".to_owned(),
        });
    }
    // Deserialize the original bytes so duplicate struct fields are rejected,
    // rather than silently overwritten while building an intermediate Value.
    // Serde data errors can quote stored values; diagnostics retain location and
    // category without copying personal metadata into logs.
    let metadata = serde_json::from_slice(bytes).map_err(|error| ProfileError::Corrupt {
        path: path.to_path_buf(),
        detail: format!(
            "invalid JSON/schema at line {}, column {} ({:?})",
            error.line(),
            error.column(),
            error.classify()
        ),
    })?;
    validate_metadata(&metadata, path)?;
    Ok(metadata)
}

fn validate_metadata(metadata: &UserMetadata, path: &Path) -> Result<(), ProfileError> {
    if metadata
        .ratings
        .keys()
        .chain(metadata.tags.keys())
        .any(String::is_empty)
    {
        return Err(ProfileError::Corrupt {
            path: path.to_path_buf(),
            detail: "stored video identifier is empty".to_owned(),
        });
    }
    if metadata
        .ratings
        .values()
        .any(|rating| !(1..=5).contains(rating))
    {
        return Err(ProfileError::Corrupt {
            path: path.to_path_buf(),
            detail: "stored rating is outside 1 through 5".to_owned(),
        });
    }
    if metadata
        .tags
        .values()
        .flatten()
        .any(|tag| normalize_tag(tag).as_deref() != Some(tag.as_str()))
    {
        return Err(ProfileError::Corrupt {
            path: path.to_path_buf(),
            detail: "stored tag is not a valid normalized label".to_owned(),
        });
    }
    Ok(())
}

fn persistence(operation: &'static str, path: &Path, source: io::Error) -> ProfileError {
    ProfileError::Persistence {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

async fn create_temporary(
    directory: &Path,
    next_temporary: &AtomicU64,
) -> Result<(PathBuf, fs::File), ProfileError> {
    loop {
        // create_new remains authoritative if the counter wraps or stale files
        // from an earlier process happen to share a candidate name.
        let sequence = next_temporary.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(".metadata.{}.{sequence}.tmp", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(file) => return Ok((path, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(persistence(
                    "create temporary profile metadata",
                    &path,
                    source,
                ));
            }
        }
    }
}

fn accumulate_rating(total: &mut (u64, u64), rating: u8) -> Result<(), ProfileError> {
    let sum = total
        .0
        .checked_add(u64::from(rating))
        .ok_or(ProfileError::AggregateOverflow)?;
    let count = total
        .1
        .checked_add(1)
        .ok_or(ProfileError::AggregateOverflow)?;
    *total = (sum, count);
    Ok(())
}

fn finish_aggregate(totals: HashMap<String, (u64, u64)>) -> HashMap<String, AggregateRating> {
    totals
        .into_iter()
        .map(|(id, (sum, count))| {
            let average = (count != 0).then(|| sum as f64 / count as f64);
            (id, AggregateRating { average, count })
        })
        .collect()
}

fn library_hash(path: &Path) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut add = |byte: u8| {
        // FNV-1a is explicitly arithmetic modulo 2^64.
        hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
    };
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        add(b'w');
        for unit in path.as_os_str().encode_wide() {
            for byte in unit.to_le_bytes() {
                add(byte);
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        add(b'u');
        for byte in path.as_os_str().as_bytes() {
            add(*byte);
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        add(b'o');
        for byte in path.as_os_str().as_encoded_bytes() {
            add(*byte);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use tokio::sync::{Barrier, oneshot};

    struct Library(PathBuf);

    impl Library {
        async fn new() -> Self {
            static NEXT_LIBRARY: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "droptube-profiles-{}-{}",
                std::process::id(),
                NEXT_LIBRARY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory)
                .await
                .expect("create isolated library");
            Self(
                fs::canonicalize(directory)
                    .await
                    .expect("canonical test library"),
            )
        }

        async fn manager(&self) -> Arc<ProfileManager> {
            Arc::new(
                ProfileManager::new(&self.0)
                    .await
                    .expect("create profile manager"),
            )
        }

        fn committed(&self, profile: &str) -> PathBuf {
            self.0
                .join(".droptube/profiles")
                .join(profile)
                .join("metadata.json")
        }

        async fn write_metadata(&self, profile: &str, bytes: &[u8]) {
            let path = self.committed(profile);
            fs::create_dir_all(path.parent().expect("metadata parent"))
                .await
                .expect("create profile directory");
            fs::write(path, bytes).await.expect("write profile fixture");
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0) {
                tracing::warn!(%error, "Could not remove isolated profile test library");
            }
        }
    }

    #[test]
    fn profile_whitelist_normalizes_alphanumeric_names_and_rejects_entire_invalid_inputs() {
        for (input, expected) in [
            ("alice", "alice"),
            ("Alice2", "alice2"),
            ("user123", "user123"),
            ("123", "123"),
            ("COM0", "com0"),
            ("LPT10", "lpt10"),
        ] {
            assert_eq!(sanitize_profile_name(input).as_deref(), Some(expected));
        }
        assert_eq!(sanitize_profile_name(&"A".repeat(32)), Some("a".repeat(32)));
        for input in [
            "",
            "a b",
            " alice",
            "alice ",
            "a\tb",
            "\talice",
            "alice\t",
            "a\nb",
            "alice\n",
            "\nalice",
            "a\rb",
            "alice_2",
            "alice-2",
            "alice.name",
            "../../etc",
            "..\\etc",
            "%2e%2e%2fetc",
            "%252e%252e",
            "a/b",
            "a\\b",
            "a\0b",
            "é",
            "Ａ",
            "alice!",
        ] {
            assert_eq!(sanitize_profile_name(input), None, "accepted {input:?}");
        }
        assert_eq!(sanitize_profile_name(&"a".repeat(33)), None);
        for input in [
            "CON", "con", "PrN", "AUX", "nul", "COM1", "com9", "LPT1", "lpt9",
        ] {
            assert_eq!(
                sanitize_profile_name(input),
                None,
                "accepted device {input}"
            );
        }
        for number in 1..=9 {
            assert_eq!(sanitize_profile_name(&format!("CoM{number}")), None);
            assert_eq!(sanitize_profile_name(&format!("LpT{number}")), None);
        }
    }

    #[test]
    fn tag_normalization_uses_unicode_character_limits_and_rejects_controls() {
        assert_eq!(
            normalize_tag("  Favorites / Sci-Fi!  ").as_deref(),
            Some("favorites / sci-fi!")
        );
        assert_eq!(
            normalize_tag("\u{2003}ÉCOLE\u{2003}").as_deref(),
            Some("école")
        );
        assert_eq!(normalize_tag(&"😀".repeat(64)), Some("😀".repeat(64)));
        assert_eq!(normalize_tag(&"😀".repeat(65)), None);
        // LATIN CAPITAL I WITH DOT lowercases to two Unicode scalar values.
        assert_eq!(normalize_tag(&"İ".repeat(33)), None);
        for input in [
            "",
            "   ",
            "\u{2003}",
            "hello\n",
            "\thello",
            "he\0llo",
            "hello\u{7f}",
            "hello\u{85}",
        ] {
            assert_eq!(normalize_tag(input), None);
        }
    }

    #[tokio::test]
    async fn reads_and_cookie_selection_never_create_storage() {
        let library = Library::new().await;
        let manager = library.manager().await;
        assert!(manager.cookie_name().starts_with("droptube_profile_"));
        assert_eq!(manager.library_root(), library.0);
        assert_eq!(
            manager.read("Alice").await.expect("empty profile"),
            UserMetadata::default()
        );
        let aggregates = manager
            .aggregate(&["film.mp4".to_owned()])
            .await
            .expect("empty aggregate");
        assert_eq!(aggregates["film.mp4"], AggregateRating::default());
        assert!(!library.0.join(".droptube").exists());
        assert_eq!(
            serde_json::to_value(UserMetadata::default()).expect("serialize empty metadata"),
            serde_json::json!({"ratings":{},"tags":{}})
        );
    }

    #[tokio::test]
    async fn invalid_profile_inputs_do_not_access_even_corrupt_storage() {
        let library = Library::new().await;
        let manager = library.manager().await;
        // Any attempted storage access would return UnsafePath instead.
        fs::write(library.0.join(".droptube"), b"not a directory")
            .await
            .expect("block storage");
        for profile in [
            "../alice",
            "..\\alice",
            "%2e%2e%2falice",
            "alice\n",
            "",
            "NUL",
        ] {
            assert!(matches!(
                manager.read(profile).await,
                Err(ProfileError::InvalidProfile)
            ));
            assert!(matches!(
                manager
                    .set_rating(profile.to_owned(), "film.mp4".to_owned(), 4)
                    .await,
                Err(ProfileError::InvalidProfile)
            ));
            assert!(matches!(
                manager
                    .add_tag(profile.to_owned(), "film.mp4".to_owned(), "tag".to_owned())
                    .await,
                Err(ProfileError::InvalidProfile)
            ));
            assert!(matches!(
                manager
                    .remove_tag(profile.to_owned(), "film.mp4".to_owned(), "tag".to_owned())
                    .await,
                Err(ProfileError::InvalidProfile)
            ));
        }
        assert_eq!(
            fs::read(library.0.join(".droptube"))
                .await
                .expect("read marker"),
            b"not a directory"
        );
    }

    #[tokio::test]
    async fn cookie_names_are_stable_for_canonical_root_and_independent_for_other_libraries() {
        let first = Library::new().await;
        let second = Library::new().await;
        let original = first.manager().await;
        let restarted = ProfileManager::new(&first.0.join("."))
            .await
            .expect("recreate manager");
        assert_eq!(original.cookie_name(), restarted.cookie_name());
        assert_ne!(original.cookie_name(), second.manager().await.cookie_name());
        assert_eq!(original.cookie_name().len(), "droptube_profile_".len() + 16);
        let suffix = original
            .cookie_name()
            .strip_prefix("droptube_profile_")
            .expect("hash suffix");
        assert!(
            suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
    }

    #[tokio::test]
    async fn first_edit_writes_clean_root_metadata_and_rerating_replaces_existing_file() {
        let library = Library::new().await;
        let manager = library.manager().await;
        let initial = manager
            .set_rating("Alice".to_owned(), "movies/film.mkv".to_owned(), 4)
            .await
            .expect("first rating");
        assert_eq!(initial.ratings["movies/film.mkv"], 4);
        let first_bytes = fs::read(library.committed("alice"))
            .await
            .expect("first committed file");
        assert!(first_bytes.starts_with(b"{\n  \"ratings\": {"));
        manager
            .add_tag(
                "alice".to_owned(),
                "movies/film.mkv".to_owned(),
                "Favorites".to_owned(),
            )
            .await
            .expect("tag edit");
        manager
            .set_rating("alice".to_owned(), "movies/film.mkv".to_owned(), 2)
            .await
            .expect("replace existing committed file");
        let committed: UserMetadata = serde_json::from_slice(
            &fs::read(library.committed("alice"))
                .await
                .expect("read replaced file"),
        )
        .expect("complete JSON");
        assert_eq!(committed.ratings["movies/film.mkv"], 2);
        assert_eq!(
            committed.tags["movies/film.mkv"],
            HashSet::from(["favorites".to_owned()])
        );
        assert!(!library.0.join("movies/.droptube").exists());
        let mut entries = fs::read_dir(library.committed("alice").parent().expect("profile dir"))
            .await
            .expect("profile entries");
        let mut names = Vec::new();
        while let Some(entry) = entries.next_entry().await.expect("next entry") {
            names.push(entry.file_name());
        }
        assert_eq!(names, [std::ffi::OsString::from("metadata.json")]);
    }

    #[tokio::test]
    async fn additive_tags_are_idempotent_and_deletions_preserve_other_labels_and_ratings() {
        let library = Library::new().await;
        let manager = library.manager().await;
        manager
            .set_rating("alice".to_owned(), "film.mp4".to_owned(), 5)
            .await
            .expect("rating");
        for label in [" Favorites ", "favorites", " SCI-FI "] {
            manager
                .add_tag("alice".to_owned(), "film.mp4".to_owned(), label.to_owned())
                .await
                .expect("add label");
        }
        let metadata = manager.read("alice").await.expect("personal tags");
        assert_eq!(metadata.tags["film.mp4"].len(), 2);
        let metadata = manager
            .remove_tag(
                "alice".to_owned(),
                "film.mp4".to_owned(),
                " FAVORITES ".to_owned(),
            )
            .await
            .expect("remove one label");
        assert_eq!(
            metadata.tags["film.mp4"],
            HashSet::from(["sci-fi".to_owned()])
        );
        let unchanged = manager
            .remove_tag(
                "alice".to_owned(),
                "film.mp4".to_owned(),
                "missing".to_owned(),
            )
            .await
            .expect("remove missing label");
        assert_eq!(metadata, unchanged);
        let empty_tags = manager
            .remove_tag(
                "alice".to_owned(),
                "film.mp4".to_owned(),
                "sci-fi".to_owned(),
            )
            .await
            .expect("remove last label");
        assert!(!empty_tags.tags.contains_key("film.mp4"));
        assert_eq!(empty_tags.ratings["film.mp4"], 5);
    }

    #[tokio::test]
    async fn library_tags_union_profiles_and_removal_preserves_other_contributions() {
        let library = Library::new().await;
        let manager = library.manager().await;
        let ids = ["film.mp4".to_owned(), "untagged.mp4".to_owned()];
        for (profile, tag) in [
            ("p1", " Favorites "),
            ("p1", "sci-fi"),
            ("p2", "favorites"),
            ("p2", "all"),
        ] {
            manager
                .add_tag(profile.to_owned(), ids[0].clone(), tag.to_owned())
                .await
                .expect("profile tag contribution");
        }
        let shared = manager
            .library_metadata(&ids)
            .await
            .expect("public library metadata needs no selected profile");
        assert_eq!(
            shared.tags[&ids[0]],
            HashSet::from([
                "favorites".to_owned(),
                "sci-fi".to_owned(),
                "all".to_owned()
            ])
        );
        assert!(!shared.tags.contains_key(&ids[1]));
        assert_eq!(shared.ratings[&ids[0]], AggregateRating::default());
        assert_eq!(shared.profiles.len(), 2);
        assert_eq!(shared.profiles[0].tag_count, 2);
        assert_eq!(shared.profiles[1].tag_count, 2);

        manager
            .remove_tag("p1".to_owned(), ids[0].clone(), "favorites".to_owned())
            .await
            .expect("remove only p1 contribution");
        assert_eq!(
            manager
                .library_metadata(&ids)
                .await
                .expect("remaining shared tags")
                .tags,
            shared.tags
        );
        assert!(
            !manager.read("p1").await.expect("p1 metadata").tags[&ids[0]].contains("favorites")
        );
        assert!(manager.read("p2").await.expect("p2 metadata").tags[&ids[0]].contains("favorites"));

        manager
            .remove_tag("p2".to_owned(), ids[0].clone(), "favorites".to_owned())
            .await
            .expect("remove final favorites contribution");
        let remaining = library
            .manager()
            .await
            .library_metadata(&ids)
            .await
            .expect("shared tags survive manager reconstruction");
        assert_eq!(
            remaining.tags[&ids[0]],
            HashSet::from(["sci-fi".to_owned(), "all".to_owned()])
        );
    }

    #[tokio::test]
    async fn library_summaries_count_only_indexed_contributions_and_omit_uncommitted_profiles() {
        let library = Library::new().await;
        let manager = library.manager().await;
        let ids = [
            "film.mp4".to_owned(),
            "second.mp4".to_owned(),
            "untagged.mp4".to_owned(),
        ];
        let empty = manager
            .library_metadata(&ids)
            .await
            .expect("empty public view");
        assert_eq!(empty.ratings.len(), 3);
        assert!(
            empty
                .ratings
                .values()
                .all(|rating| *rating == AggregateRating::default())
        );
        assert!(empty.tags.is_empty());
        assert!(empty.profiles.is_empty());
        assert!(!library.0.join(".droptube").exists());

        library.write_metadata("zeta", br#"{"ratings":{"film.mp4":4,"orphan.mp4":1},"tags":{"film.mp4":["a","shared"],"second.mp4":["shared"],"orphan.mp4":["hidden"]}}"#).await;
        library.write_metadata("alice", br#"{"ratings":{"second.mp4":5},"tags":{"second.mp4":["other"],"untagged.mp4":[]}}"#).await;
        library.write_metadata("empty", b"{}").await;
        library
            .write_metadata(
                "orphan",
                br#"{"ratings":{"orphan.mp4":2},"tags":{"orphan.mp4":["hidden"]}}"#,
            )
            .await;
        let pending_file = library.committed("pending");
        let pending_directory = pending_file.parent().expect("pending directory");
        fs::create_dir(pending_directory)
            .await
            .expect("uncommitted profile directory");
        let temporary = pending_directory.join(".metadata.123.4.tmp");
        fs::write(&temporary, b"incomplete JSON")
            .await
            .expect("uncommitted temporary file");

        let shared = manager
            .library_metadata(&ids)
            .await
            .expect("indexed public view");
        assert_eq!(
            shared.profiles,
            vec![
                ProfileSummary {
                    name: "alice".to_owned(),
                    rating_count: 1,
                    tagged_video_count: 1,
                    tag_count: 1
                },
                ProfileSummary {
                    name: "empty".to_owned(),
                    ..ProfileSummary::default()
                },
                ProfileSummary {
                    name: "orphan".to_owned(),
                    ..ProfileSummary::default()
                },
                ProfileSummary {
                    name: "zeta".to_owned(),
                    rating_count: 1,
                    tagged_video_count: 2,
                    tag_count: 2
                },
            ]
        );
        assert_eq!(
            shared.ratings[&ids[0]],
            AggregateRating {
                average: Some(4.0),
                count: 1
            }
        );
        assert_eq!(
            shared.ratings[&ids[1]],
            AggregateRating {
                average: Some(5.0),
                count: 1
            }
        );
        assert_eq!(shared.ratings[&ids[2]], AggregateRating::default());
        assert_eq!(shared.tags.len(), 2);
        assert_eq!(
            shared.tags[&ids[1]],
            HashSet::from(["shared".to_owned(), "other".to_owned()])
        );
        assert!(!shared.ratings.contains_key("orphan.mp4"));
        assert!(!shared.tags.contains_key("orphan.mp4"));
        assert_eq!(
            manager
                .aggregate(&ids)
                .await
                .expect("rating API retains its contract"),
            shared.ratings
        );
        assert!(!pending_file.exists());
        assert_eq!(
            fs::read(temporary)
                .await
                .expect("uncommitted bytes unchanged"),
            b"incomplete JSON"
        );

        let no_index = manager.library_metadata(&[]).await.expect("empty index");
        assert!(no_index.ratings.is_empty());
        assert!(no_index.tags.is_empty());
        assert!(
            no_index
                .profiles
                .iter()
                .all(|profile| profile.rating_count == 0
                    && profile.tagged_video_count == 0
                    && profile.tag_count == 0)
        );
        assert_eq!(no_index.profiles.len(), 4);
    }

    #[tokio::test]
    async fn invalid_rating_and_tag_inputs_leave_committed_bytes_unchanged() {
        let library = Library::new().await;
        let manager = library.manager().await;
        for rating in [0, 6, u8::MAX] {
            assert!(matches!(
                manager
                    .set_rating("alice".to_owned(), "film.mp4".to_owned(), rating)
                    .await,
                Err(ProfileError::InvalidRating)
            ));
        }
        for tag in [String::new(), "\nlabel".to_owned(), "x".repeat(65)] {
            assert!(matches!(
                manager
                    .add_tag("alice".to_owned(), "film.mp4".to_owned(), tag.clone())
                    .await,
                Err(ProfileError::InvalidTag)
            ));
            assert!(matches!(
                manager
                    .remove_tag("alice".to_owned(), "film.mp4".to_owned(), tag)
                    .await,
                Err(ProfileError::InvalidTag)
            ));
        }
        assert!(!library.0.join(".droptube").exists());
        manager
            .set_rating("alice".to_owned(), "film.mp4".to_owned(), 4)
            .await
            .expect("valid save");
        let before = fs::read(library.committed("alice"))
            .await
            .expect("before bytes");
        assert!(
            manager
                .set_rating("alice".to_owned(), "film.mp4".to_owned(), 0)
                .await
                .is_err()
        );
        assert!(
            manager
                .add_tag("alice".to_owned(), "film.mp4".to_owned(), "\0".to_owned())
                .await
                .is_err()
        );
        assert_eq!(
            fs::read(library.committed("alice"))
                .await
                .expect("after bytes"),
            before
        );
    }

    #[tokio::test]
    async fn independent_profiles_contribute_one_vote_and_survive_manager_reconstruction() {
        let library = Library::new().await;
        let manager = library.manager().await;
        let ids = ["film.mp4".to_owned(), "unrated.mp4".to_owned()];
        manager
            .set_rating("p1".to_owned(), ids[0].clone(), 4)
            .await
            .expect("p1 rating");
        manager
            .set_rating("p2".to_owned(), ids[0].clone(), 5)
            .await
            .expect("p2 rating");
        manager
            .add_tag("p1".to_owned(), ids[0].clone(), "favorites".to_owned())
            .await
            .expect("p1 tag");
        assert_eq!(
            manager.read("p2").await.expect("p2 personal").ratings[&ids[0]],
            5
        );
        assert!(
            manager
                .read("p2")
                .await
                .expect("p2 personal")
                .tags
                .is_empty()
        );
        assert!(
            manager
                .read("third")
                .await
                .expect("third profile")
                .ratings
                .is_empty()
        );
        let averages = manager
            .aggregate(&ids)
            .await
            .expect("initial community average");
        assert_eq!(
            averages[&ids[0]],
            AggregateRating {
                average: Some(4.5),
                count: 2
            }
        );
        assert_eq!(averages[&ids[1]], AggregateRating::default());
        manager
            .set_rating("p1".to_owned(), ids[0].clone(), 2)
            .await
            .expect("p1 rerating");
        manager
            .set_rating("p2".to_owned(), ids[0].clone(), 5)
            .await
            .expect("idempotent p2 vote");
        let restarted = library.manager().await;
        assert_eq!(
            restarted.aggregate(&ids).await.expect("restart aggregate")[&ids[0]],
            AggregateRating {
                average: Some(3.5),
                count: 2
            }
        );
        assert_eq!(
            restarted.read("p1").await.expect("restart tags").tags[&ids[0]],
            HashSet::from(["favorites".to_owned()])
        );
        manager
            .set_rating("guest".to_owned(), ids[1].clone(), 3)
            .await
            .expect("first guest");
        manager
            .set_rating("Guest".to_owned(), ids[1].clone(), 5)
            .await
            .expect("second guest client");
        assert_eq!(
            manager.aggregate(&ids).await.expect("shared guest")[&ids[1]],
            AggregateRating {
                average: Some(5.0),
                count: 1
            }
        );
    }

    #[tokio::test]
    async fn concurrent_same_profile_edits_retain_every_entry_and_label() {
        let library = Library::new().await;
        let manager = library.manager().await;
        let barrier = Arc::new(Barrier::new(24));
        let mut tasks = Vec::new();
        for number in 0..24 {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                if number < 12 {
                    manager
                        .set_rating("alice".to_owned(), format!("video{number}.mp4"), 4)
                        .await
                } else {
                    manager
                        .add_tag(
                            "Alice".to_owned(),
                            "common.mp4".to_owned(),
                            format!("label{number}"),
                        )
                        .await
                }
            }));
        }
        for task in tasks {
            task.await.expect("edit task").expect("concurrent save");
        }
        let metadata = manager
            .read("alice")
            .await
            .expect("complete personal state");
        assert_eq!(metadata.ratings.len(), 12);
        assert_eq!(metadata.tags["common.mp4"].len(), 12);
        let serialized: UserMetadata = serde_json::from_slice(
            &fs::read(library.committed("alice"))
                .await
                .expect("committed bytes"),
        )
        .expect("committed valid JSON");
        assert_eq!(metadata, serialized);
    }

    #[tokio::test]
    async fn concurrent_different_profiles_keep_every_contribution_exactly_once() {
        let library = Library::new().await;
        let manager = library.manager().await;
        let barrier = Arc::new(Barrier::new(12));
        let mut tasks = Vec::new();
        for number in 0..12 {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                manager
                    .set_rating(format!("p{number}"), "film.mp4".to_owned(), 4)
                    .await
            }));
        }
        for task in tasks {
            task.await.expect("profile task").expect("profile save");
        }
        assert_eq!(
            manager
                .aggregate(&["film.mp4".to_owned()])
                .await
                .expect("complete aggregate")["film.mp4"],
            AggregateRating {
                average: Some(4.0),
                count: 12
            }
        );
    }

    #[tokio::test]
    async fn one_profiles_transaction_lock_does_not_block_other_profiles() {
        let library = Library::new().await;
        let manager = library.manager().await;
        let lock = manager.profile_lock("alice").await;
        let _guard = lock.lock().await;
        manager
            .set_rating("bob".to_owned(), "film.mp4".to_owned(), 5)
            .await
            .expect("independent profile save");
        assert_eq!(
            manager.read("bob").await.expect("independent read").ratings["film.mp4"],
            5
        );
    }

    #[tokio::test]
    async fn dropping_a_mutation_waiter_preserves_the_owned_transaction() {
        let library = Library::new().await;
        let manager = library.manager().await;
        let lock = manager.profile_lock("alice").await;
        let guard = lock.lock().await;
        let (started_tx, started_rx) = oneshot::channel();
        let worker_manager = Arc::clone(&manager);
        let waiter = tokio::spawn(async move {
            let future = worker_manager.set_rating("alice".to_owned(), "film.mp4".to_owned(), 4);
            tokio::pin!(future);
            let mut sender = Some(started_tx);
            std::future::poll_fn(|context| {
                let poll = future.as_mut().poll(context);
                if let Some(sender) = sender.take() {
                    sender.send(()).expect("signal transaction has started");
                }
                poll
            })
            .await
        });
        started_rx.await.expect("transaction started");
        waiter.abort();
        assert!(waiter.await.expect_err("waiter cancelled").is_cancelled());
        drop(guard);
        manager.transaction_finished.notified().await;
        assert_eq!(
            manager
                .read("alice")
                .await
                .expect("detached save completed")
                .ratings["film.mp4"],
            4
        );
    }

    #[tokio::test]
    async fn corrupt_and_invalid_committed_metadata_are_preserved_and_never_partially_aggregated() {
        let library = Library::new().await;
        let manager = library.manager().await;
        manager
            .set_rating("valid".to_owned(), "film.mp4".to_owned(), 5)
            .await
            .expect("good profile");
        for bytes in [
            b"{broken".as_slice(),
            b"[]",
            b"[{},{}]",
            b"null",
            br#"{"ratings":[]}"#,
            br#"{"ratings":{},"ratings":{}}"#,
            br#"{"tags":{},"tags":{}}"#,
            br#"{"ratings":null}"#,
            br#"{"tags":null}"#,
            br#"{"ratings":{"film.mp4":"5"}}"#,
            br#"{"ratings":{"":5}}"#,
            br#"{"tags":{"":["action"]}}"#,
            br#"{"tags":{"film.mp4":"action"}}"#,
            br#"{"tags":{"film.mp4":[1]}}"#,
            br#"{"ratings":{"film.mp4":0}}"#,
            br#"{"ratings":{"film.mp4":6}}"#,
            br#"{"ratings":{"film.mp4":4.5}}"#,
            br#"{"tags":{"film.mp4":[" Not Normalized "]}}"#,
            br#"{"tags":{"film.mp4":["bad\nlabel"]}}"#,
            br#"{"tags":[]}"#,
        ] {
            library.write_metadata("corrupt", bytes).await;
            assert!(matches!(
                manager.read("corrupt").await,
                Err(ProfileError::Corrupt { .. })
            ));
            assert!(matches!(
                manager
                    .set_rating("corrupt".to_owned(), "film.mp4".to_owned(), 4)
                    .await,
                Err(ProfileError::Corrupt { .. })
            ));
            assert!(matches!(
                manager.aggregate(&["film.mp4".to_owned()]).await,
                Err(ProfileError::Corrupt { .. })
            ));
            assert!(matches!(
                manager.library_metadata(&["film.mp4".to_owned()]).await,
                Err(ProfileError::Corrupt { .. })
            ));
            assert_eq!(
                fs::read(library.committed("corrupt"))
                    .await
                    .expect("preserved corruption"),
                bytes
            );
        }
        library
            .write_metadata("corrupt", br#"{"ratings":{"orphan.mp4":3}}"#)
            .await;
        assert_eq!(
            manager
                .read("corrupt")
                .await
                .expect("missing tag field")
                .tags,
            HashMap::new()
        );
        assert_eq!(
            manager
                .aggregate(&["film.mp4".to_owned()])
                .await
                .expect("ignore orphan contribution")["film.mp4"],
            AggregateRating {
                average: Some(5.0),
                count: 1
            }
        );
        library.write_metadata("corrupt", br#"{}"#).await;
        assert_eq!(
            manager
                .read("corrupt")
                .await
                .expect("missing both map fields"),
            UserMetadata::default()
        );
    }

    #[tokio::test]
    async fn replacement_failure_preserves_prior_bytes_and_aggregates_and_cleans_current_temp() {
        let library = Library::new().await;
        let manager = library.manager().await;
        manager
            .set_rating("alice".to_owned(), "film.mp4".to_owned(), 4)
            .await
            .expect("previous commit");
        let before = fs::read(library.committed("alice"))
            .await
            .expect("prior bytes");
        manager.fail_before_replace.store(true, Ordering::Relaxed);
        assert!(matches!(
            manager
                .set_rating("alice".to_owned(), "film.mp4".to_owned(), 2)
                .await,
            Err(ProfileError::Persistence { .. })
        ));
        assert_eq!(
            fs::read(library.committed("alice"))
                .await
                .expect("preserved bytes"),
            before
        );
        assert_eq!(
            manager
                .aggregate(&["film.mp4".to_owned()])
                .await
                .expect("committed average")["film.mp4"],
            AggregateRating {
                average: Some(4.0),
                count: 1
            }
        );
        let mut entries = fs::read_dir(
            library
                .committed("alice")
                .parent()
                .expect("profile directory"),
        )
        .await
        .expect("list profile");
        assert_eq!(
            entries
                .next_entry()
                .await
                .expect("first entry")
                .expect("metadata exists")
                .file_name(),
            "metadata.json"
        );
        assert!(entries.next_entry().await.expect("next entry").is_none());
    }

    #[tokio::test]
    async fn stale_temporary_files_and_missing_committed_files_are_unrated() {
        let library = Library::new().await;
        let manager = library.manager().await;
        let directory = library
            .committed("alice")
            .parent()
            .expect("profile dir")
            .to_path_buf();
        fs::create_dir_all(&directory)
            .await
            .expect("uncommitted profile directory");
        fs::write(
            directory.join(".metadata.old.tmp"),
            b"broken temporary contents",
        )
        .await
        .expect("stale temporary");
        assert_eq!(
            manager.read("alice").await.expect("missing committed file"),
            UserMetadata::default()
        );
        assert_eq!(
            manager
                .aggregate(&["film.mp4".to_owned()])
                .await
                .expect("stale temporary ignored")["film.mp4"],
            AggregateRating::default()
        );
        fs::create_dir_all(library.0.join(".droptube/profiles/invalid-name"))
            .await
            .expect("unrelated nonprofile directory");
        fs::write(
            library
                .0
                .join(".droptube/profiles/invalid-name/metadata.json"),
            b"broken",
        )
        .await
        .expect("unrelated file");
        assert!(manager.aggregate(&[]).await.is_ok());
        let collision = directory.join(format!(".metadata.{}.0.tmp", std::process::id()));
        fs::write(&collision, b"old operation")
            .await
            .expect("temporary name collision");
        let saved = manager
            .set_rating("alice".to_owned(), "film.mp4".to_owned(), 4)
            .await
            .expect("save past stale temporary name");
        assert_eq!(saved.ratings["film.mp4"], 4);
        assert_eq!(
            fs::read(&collision).await.expect("old temporary preserved"),
            b"old operation"
        );
    }

    #[test]
    fn corrupt_metadata_diagnostics_do_not_quote_stored_values() {
        let error = decode_metadata(
            br#"{"ratings":{"film.mp4":"example-secret"}}"#,
            Path::new("metadata.json"),
        )
        .expect_err("wrong rating type");
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("line") && diagnostic.contains("column"));
        assert!(!diagnostic.contains("example-secret"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_profile_casing_aliases_contribute_through_the_normalized_identity() {
        let library = Library::new().await;
        let manager = library.manager().await;
        fs::create_dir_all(library.0.join(".droptube/profiles/ALICE"))
            .await
            .expect("existing casing alias");
        manager
            .set_rating("Alice".to_owned(), "film.mp4".to_owned(), 4)
            .await
            .expect("save through normalized profile");
        assert_eq!(
            manager
                .read("alice")
                .await
                .expect("normalized read")
                .ratings["film.mp4"],
            4
        );
        assert_eq!(
            manager
                .aggregate(&["film.mp4".to_owned()])
                .await
                .expect("normalized aggregate")["film.mp4"],
            AggregateRating {
                average: Some(4.0),
                count: 1
            }
        );
    }

    #[tokio::test]
    async fn wrong_storage_types_and_missing_libraries_return_controlled_errors() {
        let library = Library::new().await;
        assert!(matches!(
            ProfileManager::new(&library.0.join("missing")).await,
            Err(ProfileError::Persistence { .. })
        ));
        fs::write(library.0.join("file"), b"file")
            .await
            .expect("file fixture");
        assert!(matches!(
            ProfileManager::new(&library.0.join("file")).await,
            Err(ProfileError::UnsafePath(_))
        ));
        let manager = library.manager().await;
        fs::write(library.0.join(".droptube"), b"not directory")
            .await
            .expect("invalid directory type");
        assert!(matches!(
            manager.read("alice").await,
            Err(ProfileError::UnsafePath(_))
        ));
        assert!(matches!(
            manager.aggregate(&[]).await,
            Err(ProfileError::UnsafePath(_))
        ));
        fs::remove_file(library.0.join(".droptube"))
            .await
            .expect("remove marker");
        fs::remove_file(library.0.join("file"))
            .await
            .expect("remove ordinary fixture");
        fs::remove_dir(&library.0).await.expect("remove library");
        assert!(matches!(
            manager.read("alice").await,
            Err(ProfileError::Persistence { .. })
        ));
        fs::create_dir(&library.0)
            .await
            .expect("restore cleanup root");
    }

    #[tokio::test]
    async fn task_errors_and_checked_aggregate_overflow_are_explicit() {
        let task = tokio::spawn(std::future::pending::<()>());
        task.abort();
        let error: ProfileError = task.await.expect_err("cancelled task").into();
        assert!(matches!(error, ProfileError::Task(_)));
        let mut sum_overflow = (u64::MAX, 1);
        assert!(matches!(
            accumulate_rating(&mut sum_overflow, 1),
            Err(ProfileError::AggregateOverflow)
        ));
        assert_eq!(sum_overflow, (u64::MAX, 1));
        let mut count_overflow = (0, u64::MAX);
        assert!(matches!(
            accumulate_rating(&mut count_overflow, 1),
            Err(ProfileError::AggregateOverflow)
        ));
        assert_eq!(count_overflow, (0, u64::MAX));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_open_file_replacement_failure_preserves_the_existing_commit() {
        use std::os::windows::fs::OpenOptionsExt;
        let library = Library::new().await;
        let manager = library.manager().await;
        manager
            .set_rating("alice".to_owned(), "film.mp4".to_owned(), 4)
            .await
            .expect("initial commit");
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(library.committed("alice"))
            .expect("hold destination without delete sharing");
        let before = fs::read(library.committed("alice"))
            .await
            .expect("previous bytes");
        assert!(matches!(
            manager
                .set_rating("alice".to_owned(), "film.mp4".to_owned(), 2)
                .await,
            Err(ProfileError::Persistence { .. })
        ));
        assert_eq!(
            fs::read(library.committed("alice"))
                .await
                .expect("preserved bytes"),
            before
        );
        drop(held);
        manager
            .set_rating("alice".to_owned(), "film.mp4".to_owned(), 2)
            .await
            .expect("replace once handle released");
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn linked_storage_directories_never_read_or_write_outside_the_library() {
        let outside = Library::new().await;
        for component in [
            ".droptube",
            ".droptube/profiles",
            ".droptube/profiles/alice",
        ] {
            let library = Library::new().await;
            let manager = library.manager().await;
            let link = library.0.join(component);
            fs::create_dir_all(link.parent().expect("link parent"))
                .await
                .expect("parent directories");
            make_directory_link(&outside.0, &link);
            assert!(matches!(
                manager.read("alice").await,
                Err(ProfileError::UnsafePath(_))
            ));
            assert!(matches!(
                manager
                    .set_rating("alice".to_owned(), "film.mp4".to_owned(), 4)
                    .await,
                Err(ProfileError::UnsafePath(_))
            ));
            assert!(matches!(
                manager.aggregate(&[]).await,
                Err(ProfileError::UnsafePath(_))
            ));
            #[cfg(unix)]
            fs::remove_file(&link)
                .await
                .expect("remove directory symlink");
            #[cfg(windows)]
            fs::remove_dir(&link)
                .await
                .expect("remove directory junction");
        }
        assert!(
            fs::read_dir(&outside.0)
                .await
                .expect("outside directory")
                .next_entry()
                .await
                .expect("outside entries")
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn linked_committed_files_are_rejected_and_their_target_is_preserved() {
        let library = Library::new().await;
        let outside = Library::new().await;
        let target = outside.0.join("original.json");
        fs::write(&target, br#"{"ratings":{"film.mp4":5},"tags":{}}"#)
            .await
            .expect("outside metadata");
        fs::create_dir_all(library.committed("alice").parent().expect("profile dir"))
            .await
            .expect("profile directory");
        std::os::unix::fs::symlink(&target, library.committed("alice")).expect("metadata symlink");
        let manager = library.manager().await;
        assert!(matches!(
            manager.read("alice").await,
            Err(ProfileError::UnsafePath(_))
        ));
        assert!(matches!(
            manager
                .set_rating("alice".to_owned(), "film.mp4".to_owned(), 2)
                .await,
            Err(ProfileError::UnsafePath(_))
        ));
        assert!(matches!(
            manager.aggregate(&["film.mp4".to_owned()]).await,
            Err(ProfileError::UnsafePath(_))
        ));
        assert_eq!(
            fs::read(target).await.expect("preserved target"),
            br#"{"ratings":{"film.mp4":5},"tags":{}}"#
        );
    }

    #[cfg(unix)]
    fn make_directory_link(target: &Path, link: &Path) {
        std::os::unix::fs::symlink(target, link).expect("directory symlink");
    }

    #[cfg(windows)]
    fn make_directory_link(target: &Path, link: &Path) {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .creation_flags(0x0800_0000)
            .output()
            .expect("create test directory junction");
        assert!(
            output.status.success(),
            "junction creation failed: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
