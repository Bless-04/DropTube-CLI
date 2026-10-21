//! Profile identity extraction and metadata HTTP contracts. Profiles select shared data,
//! rather than authenticating users; cookie edits carry a displayed-profile precondition.

use crate::models::state::AppState;
use crate::profiles::{
    PersonalRating, ProfileError, ProfileManager, UserMetadata, sanitize_profile_name,
};
use crate::server::handlers::TemplateError;
use axum::Json;
use axum::extract::{Form, FromRequest, FromRequestParts, OriginalUri, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header, request::Parts};
use axum::response::{IntoResponse, Redirect, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

const PROFILE_HEADER: &str = "x-droptube-profile";

trait JsonPayload: DeserializeOwned {
    const INVALID_MESSAGE: &'static str;
}

/// Enforces JSON objects at the boundary; derived struct deserializers may accept arrays.
pub(crate) struct ObjectJson<T>(T);

impl<S, T> FromRequest<S> for ObjectJson<T>
where
    S: Send + Sync,
    T: JsonPayload,
{
    type Rejection = ApiError;

    async fn from_request(
        request: axum::extract::Request,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let is_json = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .is_some_and(|value| {
                let media_type = value.trim().to_ascii_lowercase();
                media_type == "application/json"
                    || (media_type.starts_with("application/") && media_type.ends_with("+json"))
            });
        if !is_json {
            return Err(ApiError::bad_request(T::INVALID_MESSAGE));
        }
        let bytes = axum::body::Bytes::from_request(request, state)
            .await
            .map_err(|_| ApiError::bad_request(T::INVALID_MESSAGE))?;
        if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
            return Err(ApiError::bad_request(T::INVALID_MESSAGE));
        }
        // Deserialize the original bytes so duplicate required fields stay detectable.
        serde_json::from_slice(&bytes)
            .map(Self)
            .map_err(|_| ApiError::bad_request(T::INVALID_MESSAGE))
    }
}

/// An HTTP failure whose public message never includes a filesystem path.
#[derive(Debug)]
pub(crate) struct ApiError {
    status: StatusCode,
    message: &'static str,
}

impl ApiError {
    pub(crate) fn bad_request(message: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    }

    pub(crate) fn internal(message: &'static str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

impl From<TemplateError> for ApiError {
    fn from(error: TemplateError) -> Self {
        tracing::error!(error = %error.0, "Could not render profile-aware page");
        Self::internal("Could not render this page")
    }
}

impl From<ProfileError> for ApiError {
    fn from(error: ProfileError) -> Self {
        match error {
            ProfileError::InvalidProfile => Self::bad_request("Invalid profile name"),
            ProfileError::InvalidRating => {
                Self::bad_request("Rating must be from 0.5 through 5 in half-star steps")
            }
            ProfileError::InvalidTag => Self::bad_request("Invalid tag label"),
            error => {
                tracing::error!(error = %error, "Could not access committed profile metadata");
                Self::internal("Profile metadata is unavailable")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProfileSource {
    Header,
    Cookie,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ResolvedProfile {
    pub(crate) name: String,
    pub(crate) source: ProfileSource,
}

/// Resolves identity without touching profile files; an explicit header is authoritative.
pub(crate) fn resolve_profile(
    headers: &HeaderMap,
    manager: &ProfileManager,
) -> Result<Option<ResolvedProfile>, ApiError> {
    let mut explicit = headers.get_all(PROFILE_HEADER).iter();
    if let Some(value) = explicit.next() {
        if explicit.next().is_some() {
            return Err(ApiError::bad_request("Repeated profile header"));
        }
        let name = value
            .to_str()
            .ok()
            .and_then(sanitize_profile_name)
            .ok_or_else(|| ApiError::bad_request("Invalid profile header"))?;
        return Ok(Some(ResolvedProfile {
            name,
            source: ProfileSource::Header,
        }));
    }
    let mut selected = None;
    for value in headers.get_all(header::COOKIE) {
        let value = value
            .to_str()
            .map_err(|_| ApiError::bad_request("Invalid profile cookie"))?;
        for segment in value.split(';') {
            let segment = segment.trim_start();
            let (name, value) = match segment.split_once('=') {
                Some(pair) => pair,
                None if segment.trim() == manager.cookie_name() => {
                    return Err(ApiError::bad_request("Malformed profile cookie"));
                }
                None => continue,
            };
            if name.trim() != manager.cookie_name() {
                continue;
            }
            if name != manager.cookie_name() || selected.is_some() {
                return Err(ApiError::bad_request(
                    "Malformed or repeated profile cookie",
                ));
            }
            let name = sanitize_profile_name(value)
                .ok_or_else(|| ApiError::bad_request("Invalid profile cookie"))?;
            selected = Some(ResolvedProfile {
                name,
                source: ProfileSource::Cookie,
            });
        }
    }
    Ok(selected)
}

/// Required validated shared-profile selection for personal metadata requests.
pub(crate) struct ActiveProfile(pub String);

impl FromRequestParts<AppState> for ActiveProfile {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let resolved = resolve_profile(&parts.headers, &state.profiles)?
            .ok_or_else(|| ApiError::bad_request("Choose a profile before editing metadata"))?;
        // Resolve once. Mutation handlers use the captured source rather than resolving again.
        parts.extensions.insert(resolved.source);
        Ok(Self(resolved.name))
    }
}

#[derive(Serialize)]
struct ProfileResponse {
    profile: Option<String>,
}

pub(crate) async fn get_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let profile = resolve_profile(&headers, &state.profiles)?.map(|profile| profile.name);
    Ok(Json(ProfileResponse { profile }).into_response())
}

#[derive(Deserialize)]
pub(crate) struct ProfileSelection {
    profile: String,
    #[serde(default)]
    return_to: Option<String>,
}

impl JsonPayload for ProfileSelection {
    const INVALID_MESSAGE: &'static str = "Invalid profile selection JSON";
}

/// Cookie selection uses its own payload, so an invalid old identifier remains recoverable.
pub(crate) async fn select_profile(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<Response, ApiError> {
    check_browser_origin(&headers, &uri)?;
    let is_form = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(';').next() == Some("application/x-www-form-urlencoded"));
    let selection = if is_form {
        let Form(selection) = Form::<ProfileSelection>::from_request(request, &state)
            .await
            .map_err(|_| ApiError::bad_request("Invalid profile selection form"))?;
        selection
    } else {
        let ObjectJson(selection) = ObjectJson::<ProfileSelection>::from_request(request, &state)
            .await
            .map_err(|_| ApiError::bad_request("Invalid profile selection JSON"))?;
        selection
    };
    let profile = sanitize_profile_name(&selection.profile)
        .ok_or_else(|| ApiError::bad_request("Invalid profile name"))?;
    let mut response = if is_form {
        Redirect::to(safe_return_to(selection.return_to.as_deref())).into_response()
    } else {
        Json(ProfileResponse {
            profile: Some(profile.clone()),
        })
        .into_response()
    };
    let secure = if uri.scheme_str() == Some("https") {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{}={profile}; Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000{secure}",
        state.profiles.cookie_name()
    );
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie)
            .map_err(|_| ApiError::internal("Could not select profile"))?,
    );
    Ok(response)
}

/// Only root-relative paths are allowed; encoded separators cannot turn a redirect into an origin.
fn safe_return_to(input: Option<&str>) -> &str {
    match input {
        Some(value)
            if value.starts_with('/')
                && !value.starts_with("//")
                && !value.contains('\\')
                && !value.chars().any(char::is_control)
                && !value
                    .split('?')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains("%2f")
                && !value
                    .split('?')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains("%5c") =>
        {
            value
        }
        _ => "/",
    }
}

/// Browser writes must come from this origin. Non-browser clients may omit browser headers.
fn check_browser_origin(headers: &HeaderMap, uri: &Uri) -> Result<(), ApiError> {
    if let Some(site) = headers.get("sec-fetch-site") {
        match site.to_str() {
            Ok("same-origin" | "none") => {}
            _ => {
                return Err(ApiError::bad_request(
                    "Cross-origin profile writes are not allowed",
                ));
            }
        }
    }
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let Some(origin) = origins.next() else {
        return Ok(());
    };
    if origins.next().is_some() {
        return Err(ApiError::bad_request("Invalid request origin"));
    }
    let origin: Uri = origin
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| ApiError::bad_request("Invalid request origin"))?;
    let scheme = uri.scheme_str().unwrap_or("http");
    let authority = uri
        .authority()
        .map(|authority| authority.as_str())
        .or_else(|| {
            headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
        })
        .ok_or_else(|| ApiError::bad_request("Missing request host"))?;
    let target: Uri = format!("{scheme}://{authority}")
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid request host"))?;
    let port = |value: &Uri| {
        value.port_u16().or_else(|| match value.scheme_str() {
            Some("http") => Some(80),
            Some("https") => Some(443),
            _ => None,
        })
    };
    if origin.scheme_str() != Some(scheme)
        || !origin
            .host()
            .zip(target.host())
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
        || port(&origin) != port(&target)
        || origin.query().is_some()
        || origin.path() != "/"
        || origin
            .authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
    {
        return Err(ApiError::bad_request(
            "Cross-origin profile writes are not allowed",
        ));
    }
    Ok(())
}

fn metadata_response(profile: &str, metadata: UserMetadata) -> Result<Response, ApiError> {
    let mut response = Json(metadata).into_response();
    response.headers_mut().insert(
        PROFILE_HEADER,
        HeaderValue::from_str(profile)
            .map_err(|_| ApiError::internal("Could not return profile metadata"))?,
    );
    Ok(response)
}

pub(crate) async fn get_metadata(
    State(state): State<AppState>,
    ActiveProfile(profile): ActiveProfile,
) -> Result<Response, ApiError> {
    metadata_response(&profile, state.profiles.read(&profile).await?)
}

#[derive(Deserialize)]
pub(crate) struct RatingMutation {
    video_id: String,
    rating: PersonalRating,
    #[serde(default, deserialize_with = "deserialize_expected_profile")]
    expected_profile: Option<String>,
}

impl JsonPayload for RatingMutation {
    const INVALID_MESSAGE: &'static str = "Invalid rating JSON";
}

#[derive(Deserialize)]
pub(crate) struct TagMutation {
    video_id: String,
    tag: String,
    #[serde(default, deserialize_with = "deserialize_expected_profile")]
    expected_profile: Option<String>,
}

/// Omission is allowed for explicit clients; a supplied precondition must be a string.
fn deserialize_expected_profile<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer).map(Some)
}

impl JsonPayload for TagMutation {
    const INVALID_MESSAGE: &'static str = "Invalid tag JSON";
}

/// Cookie-only edits are bound to the profile whose data the browser displayed.
fn check_precondition(
    profile: &str,
    source: ProfileSource,
    expected: Option<&str>,
) -> Result<(), ApiError> {
    let expected = expected
        .map(|value| {
            sanitize_profile_name(value)
                .ok_or_else(|| ApiError::bad_request("Invalid expected profile"))
        })
        .transpose()?;
    if source == ProfileSource::Cookie && expected.is_none() {
        return Err(ApiError::bad_request(
            "Expected profile is required for browser edits",
        ));
    }
    if expected.is_some_and(|expected| expected != profile) {
        return Err(ApiError {
            status: StatusCode::CONFLICT,
            message: "The active profile changed. Refresh the profile and retry your edit",
        });
    }
    Ok(())
}

async fn check_video(state: &AppState, video_id: &str) -> Result<(), ApiError> {
    if state
        .index_cache
        .read()
        .await
        .iter()
        .any(|video| video.file_name == video_id)
    {
        Ok(())
    } else {
        Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: "Video is not in the current library index",
        })
    }
}

/// Capture the write identity and browser origin before parsing a mutation body.
pub(crate) struct MutationContext {
    profile: String,
    source: ProfileSource,
}

impl FromRequestParts<AppState> for MutationContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let resolved = resolve_profile(&parts.headers, &state.profiles)?
            .ok_or_else(|| ApiError::bad_request("Choose a profile before editing metadata"))?;
        let uri = parts
            .extensions
            .get::<OriginalUri>()
            .map(|uri| &uri.0)
            .unwrap_or(&parts.uri);
        // An explicit identity does not exempt browser requests from the origin
        // boundary. API clients without browser headers remain supported.
        check_browser_origin(&parts.headers, uri)?;
        Ok(Self {
            profile: resolved.name,
            source: resolved.source,
        })
    }
}

pub(crate) async fn set_rating(
    State(state): State<AppState>,
    MutationContext { profile, source }: MutationContext,
    ObjectJson(payload): ObjectJson<RatingMutation>,
) -> Result<Response, ApiError> {
    check_precondition(&profile, source, payload.expected_profile.as_deref())?;
    check_video(&state, &payload.video_id).await?;
    let metadata = state
        .profiles
        .set_rating(profile.clone(), payload.video_id, payload.rating.stars())
        .await?;
    metadata_response(&profile, metadata)
}

pub(crate) async fn add_tag(
    State(state): State<AppState>,
    MutationContext { profile, source }: MutationContext,
    ObjectJson(payload): ObjectJson<TagMutation>,
) -> Result<Response, ApiError> {
    check_precondition(&profile, source, payload.expected_profile.as_deref())?;
    check_video(&state, &payload.video_id).await?;
    let metadata = state
        .profiles
        .add_tag(profile.clone(), payload.video_id, payload.tag)
        .await?;
    metadata_response(&profile, metadata)
}

pub(crate) async fn remove_tag(
    State(state): State<AppState>,
    MutationContext { profile, source }: MutationContext,
    ObjectJson(payload): ObjectJson<TagMutation>,
) -> Result<Response, ApiError> {
    check_precondition(&profile, source, payload.expected_profile.as_deref())?;
    check_video(&state, &payload.video_id).await?;
    let metadata = state
        .profiles
        .remove_tag(profile.clone(), payload.video_id, payload.tag)
        .await?;
    metadata_response(&profile, metadata)
}

pub(crate) async fn get_ratings(State(state): State<AppState>) -> Result<Response, ApiError> {
    let ids = state
        .index_cache
        .read()
        .await
        .iter()
        .map(|video| video.file_name.clone())
        .collect::<Vec<_>>();
    let ratings = state.profiles.aggregate(&ids).await?;
    Ok(Json(HashMap::from([("ratings", ratings)])).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::fs;
    use tokio::sync::{Mutex, RwLock};

    struct Fixture {
        root: PathBuf,
        state: AppState,
    }

    impl Fixture {
        async fn new() -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "droptube-profile-api-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).await.expect("fixture directory");
            fs::write(root.join("film.mp4"), b"video")
                .await
                .expect("video fixture");
            let root = fs::canonicalize(root).await.expect("canonical fixture");
            let state = AppState {
                movie_directory: root.clone(),
                port: 8081,
                depth: 255,
                index_cache: Arc::new(RwLock::new(Vec::new())),
                thumbnails: None,
                scan_lock: Arc::new(Mutex::new(())),
                profiles: Arc::new(ProfileManager::new(&root).await.expect("profile manager")),
            };
            state.refresh_index().await.expect("fixture index");
            Self { root, state }
        }

        fn cookie(&self, profile: &str) -> HeaderMap {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::COOKIE,
                HeaderValue::from_str(&format!("{}={profile}", self.state.profiles.cookie_name()))
                    .expect("fixture cookie"),
            );
            headers
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.root) {
                tracing::warn!(error = %error, "Could not remove profile API fixture");
            }
        }
    }

    async fn json(response: Response) -> serde_json::Value {
        serde_json::from_slice(
            &to_bytes(response.into_body(), 1024 * 1024)
                .await
                .expect("response body"),
        )
        .expect("JSON response")
    }

    async fn context(fixture: &Fixture, headers: HeaderMap) -> Result<MutationContext, ApiError> {
        let request = axum::http::Request::builder()
            .uri("/api/user/rating")
            .body(Body::empty())
            .expect("request");
        let (mut parts, _) = request.into_parts();
        parts.headers = headers;
        MutationContext::from_request_parts(&mut parts, &fixture.state).await
    }

    #[tokio::test]
    async fn identity_precedence_and_invalid_identifiers_do_not_touch_storage() {
        let fixture = Fixture::new().await;
        assert_eq!(
            resolve_profile(&HeaderMap::new(), &fixture.state.profiles).expect("optional identity"),
            None
        );
        let mut headers = fixture.cookie("Bob2");
        assert_eq!(
            resolve_profile(&headers, &fixture.state.profiles)
                .expect("cookie")
                .expect("selected"),
            ResolvedProfile {
                name: "bob2".to_owned(),
                source: ProfileSource::Cookie
            }
        );
        headers.insert(PROFILE_HEADER, HeaderValue::from_static("Alice"));
        assert_eq!(
            resolve_profile(&headers, &fixture.state.profiles)
                .expect("header")
                .expect("selected"),
            ResolvedProfile {
                name: "alice".to_owned(),
                source: ProfileSource::Header
            }
        );
        for invalid in [
            "",
            "../../etc",
            "%2e%2e%2fetc",
            "a b",
            " alice",
            "alice ",
            "alice_2",
            "alice-2",
            "NUL",
            "COM9",
        ] {
            headers.insert(
                PROFILE_HEADER,
                HeaderValue::from_str(invalid).expect("invalid but representable header"),
            );
            assert_eq!(
                resolve_profile(&headers, &fixture.state.profiles)
                    .expect_err("invalid authoritative header")
                    .status,
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                resolve_profile(&fixture.cookie(invalid), &fixture.state.profiles)
                    .expect_err("invalid cookie")
                    .status,
                StatusCode::BAD_REQUEST
            );
        }
        headers.insert(
            PROFILE_HEADER,
            HeaderValue::from_bytes(b"\xff").expect("opaque invalid header"),
        );
        assert!(resolve_profile(&headers, &fixture.state.profiles).is_err());
        assert!(!fixture.root.join(".droptube").exists());
    }

    #[tokio::test]
    async fn repeated_and_malformed_identity_fields_are_rejected() {
        let fixture = Fixture::new().await;
        let mut headers = HeaderMap::new();
        headers.append(PROFILE_HEADER, HeaderValue::from_static("alice"));
        headers.append(PROFILE_HEADER, HeaderValue::from_static("alice"));
        assert!(resolve_profile(&headers, &fixture.state.profiles).is_err());
        headers.insert(PROFILE_HEADER, HeaderValue::from_static("alice,bob"));
        assert!(resolve_profile(&headers, &fixture.state.profiles).is_err());
        for cookie in [
            format!("{0}=alice; {0}=bob", fixture.state.profiles.cookie_name()),
            format!("{} =alice", fixture.state.profiles.cookie_name()),
            fixture.state.profiles.cookie_name().to_owned(),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::COOKIE,
                HeaderValue::from_str(&cookie).expect("cookie header"),
            );
            assert!(
                resolve_profile(&headers, &fixture.state.profiles).is_err(),
                "{cookie}"
            );
        }
        let mut headers = fixture.cookie("alice");
        headers.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("{}=alice", fixture.state.profiles.cookie_name()))
                .expect("repeated cookie"),
        );
        assert!(resolve_profile(&headers, &fixture.state.profiles).is_err());
    }

    #[tokio::test]
    async fn required_identity_and_preconditions_are_validated_before_metadata_writes() {
        let fixture = Fixture::new().await;
        assert_eq!(
            context(&fixture, HeaderMap::new())
                .await
                .err()
                .expect("missing identity")
                .status,
            StatusCode::BAD_REQUEST
        );
        let response = get_metadata(
            State(fixture.state.clone()),
            ActiveProfile("alice".to_owned()),
        )
        .await
        .expect("empty read");
        assert_eq!(response.headers()[PROFILE_HEADER], "alice");
        assert_eq!(
            json(response).await,
            serde_json::json!({"ratings": {}, "tags": {}})
        );
        for (expected, status) in [
            (None, StatusCode::BAD_REQUEST),
            (Some("alice"), StatusCode::CONFLICT),
            (Some("../bob"), StatusCode::BAD_REQUEST),
        ] {
            let result = set_rating(
                State(fixture.state.clone()),
                context(&fixture, fixture.cookie("bob"))
                    .await
                    .expect("cookie context"),
                ObjectJson(RatingMutation {
                    video_id: "film.mp4".to_owned(),
                    rating: PersonalRating::new(4.0).expect("valid fixture rating"),
                    expected_profile: expected.map(str::to_owned),
                }),
            )
            .await;
            assert_eq!(result.expect_err("rejected precondition").status, status);
        }
        assert!(!fixture.root.join(".droptube").exists());
        let mut headers = HeaderMap::new();
        headers.insert(PROFILE_HEADER, HeaderValue::from_static("Alice"));
        let result = set_rating(
            State(fixture.state.clone()),
            context(&fixture, headers).await.expect("header context"),
            ObjectJson(RatingMutation {
                video_id: "film.mp4".to_owned(),
                rating: PersonalRating::new(4.0).expect("valid fixture rating"),
                expected_profile: None,
            }),
        )
        .await
        .expect("header client may omit precondition");
        assert_eq!(result.headers()[PROFILE_HEADER], "alice");
        assert_eq!(json(result).await["ratings"]["film.mp4"], 4);
    }

    #[test]
    fn expected_profile_matching_remains_required_for_explicit_clients_when_supplied() {
        assert!(check_precondition("alice", ProfileSource::Cookie, Some("Alice")).is_ok());
        assert!(check_precondition("alice", ProfileSource::Header, None).is_ok());
        assert_eq!(
            check_precondition("alice", ProfileSource::Header, Some("bob"))
                .expect_err("header precondition mismatch")
                .status,
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn browser_origin_checks_allow_same_origin_and_direct_api_requests() {
        let uri = "/api/user/rating".parse().expect("URI");
        assert!(check_browser_origin(&HeaderMap::new(), &uri).is_ok());
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("localhost:8081"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:8081"),
        );
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        assert!(check_browser_origin(&headers, &uri).is_ok());
        for origin in [
            "http://evil.example",
            "http://localhost:8082",
            "https://localhost:8081",
            "null",
            "http://localhost:8081/path",
            "http://user@localhost:8081",
        ] {
            headers.insert(
                header::ORIGIN,
                HeaderValue::from_str(origin).expect("origin"),
            );
            assert!(check_browser_origin(&headers, &uri).is_err(), "{origin}");
        }
        headers.remove(header::ORIGIN);
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-site"));
        assert!(check_browser_origin(&headers, &uri).is_err());
        headers.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(check_browser_origin(&headers, &uri).is_err());
        headers.remove("sec-fetch-site");
        headers.insert(header::HOST, HeaderValue::from_static("localhost"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:80"),
        );
        assert!(check_browser_origin(&headers, &uri).is_ok());
        headers.append(header::ORIGIN, HeaderValue::from_static("http://localhost"));
        assert!(check_browser_origin(&headers, &uri).is_err());
    }

    #[tokio::test]
    async fn browser_origin_rejection_applies_to_cookie_and_explicit_header_identities() {
        let fixture = Fixture::new().await;
        for explicit in [false, true] {
            for (header_name, header_value) in [
                (header::ORIGIN.as_str(), "https://evil.example"),
                ("sec-fetch-site", "cross-site"),
                ("sec-fetch-site", "same-site"),
            ] {
                let mut headers = fixture.cookie("bob");
                headers.insert(header::HOST, HeaderValue::from_static("localhost:8081"));
                if explicit {
                    headers.insert(PROFILE_HEADER, HeaderValue::from_static("alice"));
                }
                headers.insert(
                    axum::http::HeaderName::from_bytes(header_name.as_bytes())
                        .expect("browser header name"),
                    HeaderValue::from_static(header_value),
                );
                assert_eq!(
                    context(&fixture, headers)
                        .await
                        .err()
                        .expect("cross-origin browser request must be rejected")
                        .status,
                    StatusCode::BAD_REQUEST
                );
            }
        }
        assert!(!fixture.root.join(".droptube").exists());

        let mut headers = fixture.cookie("bob");
        headers.insert(PROFILE_HEADER, HeaderValue::from_static("alice"));
        let direct = context(&fixture, headers.clone())
            .await
            .expect("explicit API request without browser headers");
        assert_eq!(direct.profile, "alice");
        assert_eq!(direct.source, ProfileSource::Header);

        headers.insert(header::HOST, HeaderValue::from_static("localhost:8081"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:8081"),
        );
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        let same_origin = context(&fixture, headers)
            .await
            .expect("same-origin request retains explicit identity precedence");
        assert_eq!(same_origin.profile, "alice");
        assert_eq!(same_origin.source, ProfileSource::Header);
    }

    #[test]
    fn profile_form_redirects_preserve_local_queries_and_reject_external_targets() {
        for input in [
            "//evil.example",
            "https://evil.example",
            "/\\evil.example",
            "/%2fevil.example",
            "/%5cevil.example",
            "/\nLocation: http://evil.example",
        ] {
            assert_eq!(safe_return_to(Some(input)), "/");
        }
        assert_eq!(safe_return_to(None), "/");
        assert_eq!(
            safe_return_to(Some("/?v=movies%2Ffilm.mp4&search=hello")),
            "/?v=movies%2Ffilm.mp4&search=hello"
        );
        assert_eq!(safe_return_to(Some("/explorer/movies")), "/explorer/movies");
    }

    #[tokio::test]
    async fn selecting_profiles_recovers_bad_cookies_without_creating_metadata() {
        let fixture = Fixture::new().await;
        let mut headers = fixture.cookie("../../etc");
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/user/profile")
            .body(Body::from(r#"{"profile":"Alice2"}"#))
            .expect("selection request");
        *request.headers_mut() = headers.clone();
        let response = select_profile(
            State(fixture.state.clone()),
            OriginalUri("/api/user/profile".parse().expect("URI")),
            headers.clone(),
            request,
        )
        .await
        .expect("recover invalid cookie");
        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .expect("set cookie");
        assert!(cookie.starts_with(&format!("{}=alice2;", fixture.state.profiles.cookie_name())));
        for flag in ["Path=/", "HttpOnly", "SameSite=Lax", "Max-Age=31536000"] {
            assert!(cookie.contains(flag));
        }
        assert!(!cookie.contains("Domain") && !cookie.contains("Secure"));
        assert_eq!(
            json(response).await,
            serde_json::json!({"profile": "alice2"})
        );
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/user/profile")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"profile":"alice smith"}"#))
            .expect("invalid selection");
        let response = select_profile(
            State(fixture.state.clone()),
            OriginalUri("/api/user/profile".parse().expect("URI")),
            HeaderMap::new(),
            request,
        )
        .await
        .expect_err("invalid profile")
        .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!response.headers().contains_key(header::SET_COOKIE));
        assert_eq!(
            json(response).await,
            serde_json::json!({"error":"Invalid profile name"})
        );
        assert!(!fixture.root.join(".droptube").exists());
    }

    #[tokio::test]
    async fn html_profile_selection_uses_safe_303_and_https_cookies_are_secure() {
        let fixture = Fixture::new().await;
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/user/profile")
            .body(Body::from("profile=Guest&return_to=%2F%3Fv%3Dfilm.mp4"))
            .expect("form request");
        *request.headers_mut() = headers.clone();
        let response = select_profile(
            State(fixture.state.clone()),
            OriginalUri(
                "https://localhost:8081/api/user/profile"
                    .parse()
                    .expect("HTTPS URI"),
            ),
            headers,
            request,
        )
        .await
        .expect("form selection");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/?v=film.mp4");
        assert!(
            response.headers()[header::SET_COOKIE]
                .to_str()
                .expect("cookie")
                .ends_with("; Secure")
        );
        assert!(!fixture.root.join(".droptube").exists());
    }

    #[tokio::test]
    async fn profile_reads_expose_only_the_selection_and_metadata_errors_are_json() {
        let fixture = Fixture::new().await;
        assert_eq!(
            json(
                get_profile(State(fixture.state.clone()), HeaderMap::new())
                    .await
                    .expect("no selection")
            )
            .await,
            serde_json::json!({"profile":null})
        );
        assert_eq!(
            json(
                get_profile(State(fixture.state.clone()), fixture.cookie("Alice"))
                    .await
                    .expect("selection")
            )
            .await,
            serde_json::json!({"profile":"alice"})
        );
        let selected = context(&fixture, fixture.cookie("alice"))
            .await
            .expect("context");
        let error = set_rating(
            State(fixture.state.clone()),
            selected,
            ObjectJson(RatingMutation {
                video_id: "../film.mp4".to_owned(),
                rating: PersonalRating::new(4.0).expect("valid fixture rating"),
                expected_profile: Some("alice".to_owned()),
            }),
        )
        .await
        .expect_err("unknown indexed ID");
        assert_eq!(error.status, StatusCode::NOT_FOUND);
        assert!(json(error.into_response()).await["error"].is_string());
        assert!(!fixture.root.join(".droptube").exists());
        fs::create_dir_all(fixture.root.join(".droptube/profiles/alice"))
            .await
            .expect("corruption fixture");
        fs::write(
            fixture.root.join(".droptube/profiles/alice/ratings.json"),
            b"invalid-json",
        )
        .await
        .expect("corrupt fixture");
        let response = get_metadata(
            State(fixture.state.clone()),
            ActiveProfile("alice".to_owned()),
        )
        .await
        .expect_err("corrupt metadata")
        .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = json(response).await;
        assert_eq!(
            body,
            serde_json::json!({"error":"Profile metadata is unavailable"})
        );
        assert_eq!(
            fs::read(fixture.root.join(".droptube/profiles/alice/ratings.json"))
                .await
                .expect("preserved bytes"),
            b"invalid-json"
        );
    }

    #[tokio::test]
    async fn aggregate_reads_are_public_and_only_expose_averages_and_counts() {
        let fixture = Fixture::new().await;
        assert_eq!(
            json(
                get_ratings(State(fixture.state.clone()))
                    .await
                    .expect("public unrated aggregate")
            )
            .await,
            serde_json::json!({"ratings":{"film.mp4":{"average":null,"count":0}}})
        );
        fixture
            .state
            .profiles
            .set_rating("alice".to_owned(), "film.mp4".to_owned(), 4)
            .await
            .expect("alice vote");
        fixture
            .state
            .profiles
            .set_rating("bob".to_owned(), "film.mp4".to_owned(), 5)
            .await
            .expect("bob vote");
        assert_eq!(
            json(
                get_ratings(State(fixture.state.clone()))
                    .await
                    .expect("public community aggregate")
            )
            .await,
            serde_json::json!({"ratings":{"film.mp4":{"average":4.5,"count":2}}})
        );
    }

    #[tokio::test]
    async fn tag_endpoints_return_canonical_complete_metadata_and_preserve_ratings() {
        let fixture = Fixture::new().await;
        fixture
            .state
            .profiles
            .set_rating("alice".to_owned(), "film.mp4".to_owned(), 4)
            .await
            .expect("vote");
        for label in [" Favorites ", "sci-fi", "favorites"] {
            let response = add_tag(
                State(fixture.state.clone()),
                context(&fixture, fixture.cookie("alice"))
                    .await
                    .expect("context"),
                ObjectJson(TagMutation {
                    video_id: "film.mp4".to_owned(),
                    tag: label.to_owned(),
                    expected_profile: Some("alice".to_owned()),
                }),
            )
            .await
            .expect("add canonical tag");
            assert_eq!(response.headers()[PROFILE_HEADER], "alice");
            assert_eq!(json(response).await["ratings"]["film.mp4"], 4);
        }
        let response = remove_tag(
            State(fixture.state.clone()),
            context(&fixture, fixture.cookie("alice"))
                .await
                .expect("context"),
            ObjectJson(TagMutation {
                video_id: "film.mp4".to_owned(),
                tag: " FAVORITES ".to_owned(),
                expected_profile: Some("alice".to_owned()),
            }),
        )
        .await
        .expect("remove only tag");
        let body = json(response).await;
        assert_eq!(body["tags"]["film.mp4"], serde_json::json!(["sci-fi"]));
        assert_eq!(body["ratings"]["film.mp4"], 4);
    }

    #[tokio::test]
    async fn malformed_mutation_json_is_returned_as_uniform_400() {
        let fixture = Fixture::new().await;
        for input in [
            "{}",
            "null",
            r#"["film.mp4",4,"alice"]"#,
            r#"{"video_id":"film.mp4","rating":4,"rating":5}"#,
            r#"{"video_id":"film.mp4","rating":4,"expected_profile":"alice","expected_profile":"bob"}"#,
            r#"{"video_id":"film.mp4","rating":4,"expected_profile":null}"#,
            r#"{"video_id":"film.mp4","rating":4.25}"#,
            r#"{"video_id":"film.mp4","rating":-1}"#,
            r#"{"video_id":"film.mp4","rating":"4"}"#,
            "not-json",
        ] {
            let request = axum::http::Request::builder()
                .method("POST")
                .uri("/api/user/rating")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(input))
                .expect("invalid JSON request");
            let response = ObjectJson::<RatingMutation>::from_request(request, &fixture.state)
                .await
                .err()
                .expect("malformed request")
                .into_response();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert_eq!(
                json(response).await,
                serde_json::json!({"error":"Invalid rating JSON"})
            );
        }
        assert!(!fixture.root.join(".droptube").exists());
    }

    #[tokio::test]
    async fn explicit_header_clients_may_omit_but_cannot_null_a_profile_precondition() {
        let fixture = Fixture::new().await;
        for input in [
            r#"{"video_id":"film.mp4","rating":4}"#,
            r#"{"video_id":"film.mp4","rating":4,"expected_profile":"alice"}"#,
        ] {
            let request = axum::http::Request::builder()
                .method("POST")
                .uri("/api/user/rating")
                .header(header::CONTENT_TYPE, "application/json")
                .header(PROFILE_HEADER, "alice")
                .body(Body::from(input))
                .expect("explicit client request");
            let (mut parts, body) = request.into_parts();
            let context = MutationContext::from_request_parts(&mut parts, &fixture.state)
                .await
                .expect("explicit identity");
            assert_eq!(context.source, ProfileSource::Header);
            let ObjectJson(payload) = ObjectJson::<RatingMutation>::from_request(
                axum::http::Request::from_parts(parts, body),
                &fixture.state,
            )
            .await
            .expect("valid explicit precondition payload");
            assert!(
                check_precondition(
                    &context.profile,
                    context.source,
                    payload.expected_profile.as_deref()
                )
                .is_ok()
            );
        }
        for (path, input) in [
            (
                "/api/user/rating",
                r#"{"video_id":"film.mp4","rating":4,"expected_profile":null}"#,
            ),
            (
                "/api/user/tag",
                r#"{"video_id":"film.mp4","tag":"favorites","expected_profile":null}"#,
            ),
        ] {
            let request = axum::http::Request::builder()
                .method("POST")
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .header(PROFILE_HEADER, "alice")
                .body(Body::from(input))
                .expect("explicit null request");
            let (mut parts, body) = request.into_parts();
            let context = MutationContext::from_request_parts(&mut parts, &fixture.state)
                .await
                .expect("explicit identity");
            assert_eq!(context.source, ProfileSource::Header);
            let request = axum::http::Request::from_parts(parts, body);
            let error = if path.ends_with("rating") {
                ObjectJson::<RatingMutation>::from_request(request, &fixture.state)
                    .await
                    .err()
                    .expect("null rating precondition")
            } else {
                ObjectJson::<TagMutation>::from_request(request, &fixture.state)
                    .await
                    .err()
                    .expect("null tag precondition")
            };
            assert_eq!(error.status, StatusCode::BAD_REQUEST);
        }
        assert!(!fixture.root.join(".droptube").exists());
    }
}
