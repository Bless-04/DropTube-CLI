use crate::config::constants::PAGE_SIZE;
use crate::models::state::{AppState, HomeQuery};
use crate::models::video::{VideoFile, VideoFormat};
use crate::profiles::{AggregateRating, LibraryMetadata, UserMetadata};
use crate::server::handlers::TemplateError;
use crate::server::handlers::profiles::resolve_profile;
use askama::Template;
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Response},
};
use local_ip_address::local_ip;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use std::collections::{HashMap, HashSet};

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    search_query: String,
    tag_filter: String,
    tags: Vec<TagLink>,
    all_url: String,
    active: Option<VideoCard>,
    cards: Vec<VideoCard>,
    total_count: usize,
    current_page: usize,
    total_pages: usize,
    previous_url: String,
    next_url: String,
    has_filters: bool,
    local_ip: String,
    port: u16,
    active_profile: Option<String>,
    profile_input: String,
    return_to: String,
    metadata_available: bool,
    library_metadata_available: bool,
}
struct TagLink {
    label: String,
    url: String,
    selected: bool,
}
struct VideoCard {
    video_id: String,
    title: String,
    watch_url: String,
    media_url: String,
    thumbnail_url: Option<String>,
    format: &'static str,
    size: String,
    timestamp: u64,
    tags: Vec<String>,
    personal_tags: Vec<String>,
    community_rating: String,
    personal_rating: u8,
    is_playing: bool,
    is_mkv: bool,
}
struct PageData<'a> {
    videos: &'a [VideoFile],
    metadata: &'a UserMetadata,
    library_metadata: Option<&'a LibraryMetadata>,
    metadata_available: bool,
    profile: Option<String>,
    query: HomeQuery,
    port: u16,
}

fn rating_label(rating: Option<&AggregateRating>, available: bool) -> String {
    if !available {
        return "Rating unavailable".to_owned();
    }
    match rating {
        Some(AggregateRating {
            average: Some(average),
            count,
        }) => {
            // Match browser display rounding; keep the derived contribution average unrounded.
            let displayed_average = (average * 10.0).round() / 10.0;
            format!(
                "{displayed_average:.1}/5 · {count} {}",
                if *count == 1 { "rating" } else { "ratings" }
            )
        }
        _ => "Unrated".to_owned(),
    }
}
impl VideoCard {
    fn new(video: &VideoFile, query: &HomeQuery, data: &PageData<'_>) -> Self {
        let mut personal_tags: Vec<_> = data
            .metadata
            .tags
            .get(&video.file_name)
            .map(|tags| tags.iter().cloned().collect())
            .unwrap_or_default();
        personal_tags.sort();
        let mut tags: Vec<_> = data
            .library_metadata
            .and_then(|metadata| metadata.tags.get(&video.file_name))
            .map(|tags| tags.iter().cloned().collect())
            .unwrap_or_default();
        tags.sort();
        Self {
            video_id: video.file_name.clone(),
            title: video.display_name.clone(),
            watch_url: page_url(
                query,
                query.page.unwrap_or(1) as usize,
                Some(&video.file_name),
            ),
            media_url: media_url(&video.file_name),
            thumbnail_url: video.thumbnail_path.as_deref().map(media_url),
            format: video.format.as_str(),
            size: if video.file_size_mb >= 1024 {
                format!("{:.1} GB", video.file_size_mb as f64 / 1024.0)
            } else if video.file_size_mb == 0 {
                "<1 MB".to_owned()
            } else {
                format!("{} MB", video.file_size_mb)
            },
            timestamp: video.unix_timestamp,
            tags,
            personal_tags,
            community_rating: rating_label(
                data.library_metadata
                    .and_then(|metadata| metadata.ratings.get(&video.file_name)),
                data.library_metadata.is_some(),
            ),
            personal_rating: data
                .metadata
                .ratings
                .get(&video.file_name)
                .copied()
                .unwrap_or(0),
            is_playing: query.v.as_deref() == Some(video.file_name.as_str()),
            is_mkv: video.format == VideoFormat::Mkv,
        }
    }
}
fn media_url(path: &str) -> String {
    format!(
        "/video/{}",
        path.split('/')
            .map(|part| utf8_percent_encode(part, NON_ALPHANUMERIC).to_string())
            .collect::<Vec<_>>()
            .join("/")
    )
}
fn page_url(query: &HomeQuery, page: usize, active: Option<&str>) -> String {
    let mut url = format!("/?page={page}");
    for (key, value) in [
        ("search", query.search.as_deref()),
        ("tag", query.tag.as_deref()),
        ("v", active),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            url.push_str(&format!(
                "&{key}={}",
                utf8_percent_encode(value, NON_ALPHANUMERIC)
            ));
        }
    }
    url
}
fn matches_search(
    video: &VideoFile,
    terms: &[String],
    tags: &HashMap<String, HashSet<String>>,
) -> bool {
    let mut searchable = format!("{} {}", video.display_name, video.file_name).to_lowercase();
    if let Some(tags) = tags.get(&video.file_name) {
        for tag in tags {
            searchable.push(' ');
            searchable.push_str(tag);
        }
    }
    terms.iter().all(|term| searchable.contains(term))
}
impl HomeTemplate {
    fn new(mut data: PageData<'_>) -> Self {
        let search_query = data
            .query
            .search
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_owned();
        let empty_tags = HashMap::new();
        let shared_tags = data
            .library_metadata
            .map_or(&empty_tags, |metadata| &metadata.tags);
        // Preserve requested filters on a failed snapshot: widening to the full library
        // would incorrectly present an unavailable tag result as a successful match.
        let tag_filter = data
            .query
            .tag
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        data.query.search = Some(search_query.clone());
        data.query.tag = Some(tag_filter.clone());
        let terms: Vec<_> = search_query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let filtered: Vec<_> = data
            .videos
            .iter()
            .filter(|video| {
                matches_search(video, &terms, shared_tags)
                    && (tag_filter.is_empty()
                        || shared_tags
                            .get(&video.file_name)
                            .is_some_and(|tags| tags.contains(&tag_filter)))
            })
            .collect();
        let total_count = filtered.len();
        let total_pages = total_count.div_ceil(PAGE_SIZE).max(1);
        let current_page = (data.query.page.unwrap_or(1) as usize).clamp(1, total_pages);
        data.query.page = Some(current_page as u32);
        let active = data
            .query
            .v
            .as_deref()
            .and_then(|id| data.videos.iter().find(|video| video.file_name == id))
            .map(|video| VideoCard::new(video, &data.query, &data));
        let cards = filtered
            .into_iter()
            .skip((current_page - 1) * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|video| VideoCard::new(video, &data.query, &data))
            .collect();
        let mut unique_tags: Vec<_> = data
            .videos
            .iter()
            .filter_map(|video| shared_tags.get(&video.file_name))
            .flat_map(|tags| tags.iter().cloned())
            .collect();
        unique_tags.sort();
        unique_tags.dedup();
        let tags = unique_tags
            .into_iter()
            .map(|label| {
                let query = HomeQuery {
                    search: Some(search_query.clone()),
                    tag: Some(label.clone()),
                    page: None,
                    v: data.query.v.clone(),
                };
                TagLink {
                    selected: label == tag_filter,
                    url: page_url(&query, 1, query.v.as_deref()),
                    label,
                }
            })
            .collect();
        let all_url = page_url(
            &HomeQuery {
                search: Some(search_query.clone()),
                tag: None,
                page: None,
                v: data.query.v.clone(),
            },
            1,
            data.query.v.as_deref(),
        );
        let return_to = page_url(&data.query, current_page, data.query.v.as_deref());
        Self {
            previous_url: page_url(
                &data.query,
                current_page.saturating_sub(1).max(1),
                data.query.v.as_deref(),
            ),
            next_url: page_url(
                &data.query,
                current_page.saturating_add(1).min(total_pages),
                data.query.v.as_deref(),
            ),
            has_filters: !search_query.is_empty() || !tag_filter.is_empty(),
            search_query,
            tag_filter,
            tags,
            all_url,
            active,
            cards,
            total_count,
            current_page,
            total_pages,
            local_ip: local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|_| "localhost".to_owned()),
            port: data.port,
            profile_input: data.profile.clone().unwrap_or_else(|| "guest".to_owned()),
            active_profile: data.profile,
            return_to,
            metadata_available: data.metadata_available,
            library_metadata_available: data.library_metadata.is_some(),
        }
    }
}
/// Renders shared community metadata and personal editing controls without loading video bytes.
pub async fn home_page_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HomeQuery>,
) -> Result<Response, TemplateError> {
    let profile = match resolve_profile(&headers, &state.profiles) {
        Ok(profile) => profile.map(|profile| profile.name),
        Err(error) => return Ok(error.into_response()),
    };
    let videos = state.index_cache.read().await.clone();
    let ids: Vec<_> = videos.iter().map(|video| video.file_name.clone()).collect();
    let (metadata, metadata_available) = match profile.as_deref() {
        Some(profile) => match state.profiles.read(profile).await {
            Ok(metadata) => (metadata, true),
            Err(error) => {
                tracing::error!(error = %error, "Personal metadata unavailable");
                (UserMetadata::default(), false)
            }
        },
        None => (UserMetadata::default(), true),
    };
    let library_metadata = match state.profiles.library_metadata(&ids).await {
        Ok(metadata) => Some(metadata),
        Err(error) => {
            tracing::error!(error = %error, "Community ratings and shared tags unavailable");
            None
        }
    };
    let template = HomeTemplate::new(PageData {
        videos: &videos,
        metadata: &metadata,
        library_metadata: library_metadata.as_ref(),
        metadata_available,
        profile,
        query,
        port: state.port,
    });
    Ok(Html(template.render()?).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    fn video(name: &str) -> VideoFile {
        VideoFile {
            file_name: format!("courses/{name}.mp4"),
            display_name: name.to_owned(),
            file_size_mb: 12,
            unix_timestamp: 1,
            format: VideoFormat::Mp4,
            thumbnail_path: Some("courses/a & b.jpg".to_owned()),
        }
    }
    fn query(search: &str) -> HomeQuery {
        HomeQuery {
            search: Some(search.to_owned()),
            tag: None,
            page: None,
            v: None,
        }
    }
    fn page(
        videos: &[VideoFile],
        query: HomeQuery,
        metadata: &UserMetadata,
        profile: Option<String>,
    ) -> HomeTemplate {
        let library_metadata = LibraryMetadata {
            tags: metadata.tags.clone(),
            ..LibraryMetadata::default()
        };
        HomeTemplate::new(PageData {
            videos,
            metadata,
            library_metadata: Some(&library_metadata),
            metadata_available: true,
            profile,
            query,
            port: 8081,
        })
    }
    fn metadata(videos: &[VideoFile]) -> UserMetadata {
        UserMetadata {
            ratings: HashMap::new(),
            tags: videos
                .iter()
                .map(|video| {
                    (
                        video.file_name.clone(),
                        HashSet::from(["technology".to_owned()]),
                    )
                })
                .collect(),
        }
    }
    #[test]
    fn search_matches_all_terms_across_title_filename_and_shared_tags() {
        let videos = vec![video("Rust Basics"), video("Cooking")];
        let metadata = metadata(&videos);
        let template = page(
            &videos,
            query("  RUST courses technology mp4  "),
            &metadata,
            Some("alice".to_owned()),
        );
        assert_eq!(template.total_count, 1);
        assert_eq!(template.cards[0].title, "Rust Basics");
        assert_eq!(
            page(
                &videos,
                query("technology"),
                &UserMetadata::default(),
                Some("bob".to_owned())
            )
            .total_count,
            0
        );
    }
    #[test]
    fn filtering_precedes_pagination_and_arbitrary_all_tag_is_filterable() {
        let videos: Vec<_> = (0..31).map(|i| video(&format!("Video {i}"))).collect();
        let mut metadata = metadata(&videos);
        metadata.tags.insert(
            videos[30].file_name.clone(),
            HashSet::from(["all".to_owned()]),
        );
        let mut search = query("");
        search.page = Some(u32::MAX);
        search.tag = Some("ALL".to_owned());
        let template = page(&videos, search, &metadata, Some("alice".to_owned()));
        assert_eq!(template.total_count, 1);
        assert_eq!(template.current_page, 1);
        assert_eq!(template.cards[0].video_id, videos[30].file_name);
        let mut second = query("video");
        second.page = Some(2);
        assert_eq!(
            page(&videos, second, &metadata, Some("alice".to_owned()))
                .cards
                .len(),
            7
        );
    }
    #[test]
    fn unselected_profile_filters_shared_tags_and_prefills_guest() {
        let videos = vec![video("First")];
        let mut search = query("");
        search.tag = Some("technology".to_owned());
        let template = page(&videos, search, &metadata(&videos), None);
        assert_eq!(template.total_count, 1);
        assert_eq!(template.tag_filter, "technology");
        let html = template.render().expect("render page");
        assert!(html.contains("value=\"guest\""));
        assert!(html.contains("data-profile-selected=\"false\""));
    }
    #[test]
    fn feed_escapes_metadata_and_preserves_lazy_media_contract() {
        let videos = vec![video("O'Brien & <script>alert(1)</script>")];
        let html = page(
            &videos,
            query(""),
            &metadata(&videos),
            Some("alice".to_owned()),
        )
        .render()
        .expect("render feed");
        assert!(!html.contains("<video"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("loading=\"lazy\""));
        assert!(html.contains("data-src=\"/video/courses/a%20%26%20b%2Ejpg\""));
    }
    #[test]
    fn watch_render_separates_personal_rating_from_community_and_preserves_filters() {
        let videos = vec![video("First"), video("Second")];
        let mut metadata = metadata(&videos);
        metadata.ratings.insert(videos[1].file_name.clone(), 5);
        let aggregates = HashMap::from([(
            videos[1].file_name.clone(),
            AggregateRating {
                average: Some(4.5),
                count: 2,
            },
        )]);
        let mut search = query("technology");
        search.tag = Some("technology".to_owned());
        search.v = Some(videos[1].file_name.clone());
        let library_metadata = LibraryMetadata {
            ratings: aggregates,
            tags: metadata.tags.clone(),
            ..LibraryMetadata::default()
        };
        let template = HomeTemplate::new(PageData {
            videos: &videos,
            metadata: &metadata,
            library_metadata: Some(&library_metadata),
            metadata_available: true,
            profile: Some("p2".to_owned()),
            query: search,
            port: 8081,
        });
        assert!(
            template.cards[0]
                .watch_url
                .contains("search=technology&tag=technology")
        );
        let html = template.render().expect("render watch");
        assert_eq!(html.matches("<video").count(), 1);
        assert!(html.contains("4.5/5 · 2 ratings"));
        assert!(html.contains("Your rating"));
        assert!(html.contains("data-rating=\"5\""));
        assert!(html.contains("aria-label=\"Rate 5 out of 5 stars\" aria-pressed=\"true\""));
        assert!(!html.contains("Save rating") && !html.contains("<select"));
        assert!(html.contains("src=\"/video/courses/Second%2Emp4\""));
    }
    #[test]
    fn unavailable_metadata_disables_edits_and_failed_aggregation_keeps_player() {
        let videos = vec![video("First")];
        let mut search = query("");
        search.v = Some(videos[0].file_name.clone());
        let template = HomeTemplate::new(PageData {
            videos: &videos,
            metadata: &UserMetadata::default(),
            library_metadata: None,
            metadata_available: false,
            profile: Some("alice".to_owned()),
            query: search,
            port: 8081,
        });
        let html = template.render().expect("render unavailable");
        assert!(html.contains("Rating unavailable"));
        assert!(html.contains("Personal metadata unavailable"));
        assert!(html.contains("id=\"video-player\""));
    }
    #[test]
    fn shared_tags_are_visible_without_selection_and_personal_removals_remain_separate() {
        let videos = vec![video("First")];
        let personal = UserMetadata {
            tags: HashMap::from([(
                videos[0].file_name.clone(),
                HashSet::from(["mine".to_owned()]),
            )]),
            ..UserMetadata::default()
        };
        let shared = LibraryMetadata {
            tags: HashMap::from([(
                videos[0].file_name.clone(),
                HashSet::from(["mine".to_owned(), "theirs".to_owned()]),
            )]),
            ..LibraryMetadata::default()
        };
        let mut search = query("theirs");
        search.tag = Some("theirs".to_owned());
        search.v = Some(videos[0].file_name.clone());
        let template = HomeTemplate::new(PageData {
            videos: &videos,
            metadata: &personal,
            library_metadata: Some(&shared),
            metadata_available: true,
            profile: Some("alice".to_owned()),
            query: search,
            port: 8081,
        });
        assert_eq!(template.total_count, 1);
        let html = template.render().expect("shared watch view");
        assert!(html.contains("All tags") && html.contains("Your tags"));
        assert!(html.contains("class=\"shared-tag\">theirs</span>"));
        assert!(html.contains("data-remove-tag=\"mine\""));
        assert!(!html.contains("data-remove-tag=\"theirs\""));
    }
    #[test]
    fn failed_shared_snapshot_preserves_requested_filter_and_reports_unavailable_results() {
        let videos = vec![video("First")];
        let mut search = query("");
        search.tag = Some("favorites".to_owned());
        search.v = Some(videos[0].file_name.clone());
        let template = HomeTemplate::new(PageData {
            videos: &videos,
            metadata: &UserMetadata::default(),
            library_metadata: None,
            metadata_available: true,
            profile: Some("alice".to_owned()),
            query: search,
            port: 8081,
        });
        assert_eq!(template.total_count, 0);
        assert_eq!(template.tag_filter, "favorites");
        assert!(template.return_to.contains("tag=favorites"));
        let html = template.render().expect("unavailable shared view");
        assert!(html.contains("Tag results unavailable"));
        assert!(html.contains("Tags unavailable"));
        assert!(html.contains("Rating unavailable"));
        assert!(html.contains("id=\"video-player\""));
        assert!(!html.contains("No videos match these filters"));
        assert!(!html.contains("Personal metadata unavailable"));
    }
    #[test]
    fn empty_library_and_empty_search_have_distinct_guidance() {
        assert!(
            page(&[], query(""), &UserMetadata::default(), None)
                .render()
                .expect("empty")
                .contains("Your library is ready for videos")
        );
        assert!(
            page(
                &[video("First")],
                query("missing"),
                &UserMetadata::default(),
                None
            )
            .render()
            .expect("filtered")
            .contains("No videos found")
        );
        assert_eq!(
            rating_label(
                Some(&AggregateRating {
                    average: Some(5.0),
                    count: 1
                }),
                true
            ),
            "5.0/5 · 1 rating"
        );
    }

    #[test]
    fn community_display_rounds_exact_ties_upward_without_changing_the_average() {
        let rating = AggregateRating {
            average: Some(4.25),
            count: 4,
        };
        assert_eq!(rating_label(Some(&rating), true), "4.3/5 · 4 ratings");
        assert_eq!(rating.average, Some(4.25));
    }
}
