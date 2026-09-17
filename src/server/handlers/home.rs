use crate::config::constants::PAGE_SIZE;
use crate::models::state::{AppState, HomeQuery};
use crate::models::video::{Tag, VideoFile, VideoFormat};
use crate::server::handlers::TemplateError;
use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
};
use local_ip_address::local_ip;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

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
}

struct TagLink {
    label: String,
    url: String,
    selected: bool,
}

struct VideoCard {
    title: String,
    watch_url: String,
    media_url: String,
    thumbnail_url: Option<String>,
    format: &'static str,
    size: String,
    timestamp: u64,
    tags: Vec<String>,
    rating: &'static str,
    is_rated: bool,
    is_playing: bool,
    is_mkv: bool,
}

impl VideoCard {
    fn new(video: &VideoFile, query: &HomeQuery) -> Self {
        Self {
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
            tags: video
                .tags
                .iter()
                .map(|tag| tag.as_str().to_owned())
                .collect(),
            rating: video.rating.as_stars(),
            is_rated: video.rating.is_rated(),
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

fn matches_search(video: &VideoFile, terms: &[String]) -> bool {
    let mut searchable = format!("{} {}", video.display_name, video.file_name).to_lowercase();
    for tag in &video.tags {
        searchable.push(' ');
        searchable.push_str(&tag.as_str().to_lowercase());
    }
    terms.iter().all(|term| searchable.contains(term))
}

impl HomeTemplate {
    fn new(videos: &[VideoFile], mut query: HomeQuery, port: u16) -> Self {
        let search_query = query
            .search
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_owned();
        let tag_filter = query.tag.as_deref().unwrap_or_default().trim();
        let tag_filter = if tag_filter.eq_ignore_ascii_case("all") {
            String::new()
        } else {
            tag_filter.to_owned()
        };
        query.search = Some(search_query.clone());
        query.tag = Some(tag_filter.clone());
        let selected_tag = Tag::parse(&tag_filter);
        let terms: Vec<_> = search_query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let filtered: Vec<_> = videos
            .iter()
            .filter(|video| {
                matches_search(video, &terms)
                    && (tag_filter.is_empty() || video.tags.contains(&selected_tag))
            })
            .collect();
        let total_count = filtered.len();
        let total_pages = total_count.div_ceil(PAGE_SIZE).max(1);
        let current_page = (query.page.unwrap_or(1) as usize).clamp(1, total_pages);
        query.page = Some(current_page as u32);
        let active = query
            .v
            .as_deref()
            .and_then(|name| videos.iter().find(|video| video.file_name == name))
            .map(|video| VideoCard::new(video, &query));
        let cards = filtered
            .into_iter()
            .skip((current_page - 1) * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|video| VideoCard::new(video, &query))
            .collect();
        let mut unique_tags: Vec<_> = videos
            .iter()
            .flat_map(|video| video.tags.iter().cloned())
            .collect();
        unique_tags.sort_by_key(|tag| tag.as_str().to_lowercase());
        unique_tags.dedup();
        let tags = unique_tags
            .into_iter()
            .map(|tag| {
                let tag_query = HomeQuery {
                    search: Some(search_query.clone()),
                    tag: Some(tag.as_str().to_owned()),
                    page: None,
                    v: None,
                };
                TagLink {
                    label: tag.as_str().to_owned(),
                    url: page_url(&tag_query, 1, None),
                    selected: !tag_filter.is_empty() && tag == selected_tag,
                }
            })
            .collect();
        let all_url = page_url(
            &HomeQuery {
                search: Some(search_query.clone()),
                tag: None,
                page: None,
                v: None,
            },
            1,
            None,
        );
        Self {
            previous_url: page_url(
                &query,
                current_page.saturating_sub(1).max(1),
                query.v.as_deref(),
            ),
            next_url: page_url(
                &query,
                current_page.saturating_add(1).min(total_pages),
                query.v.as_deref(),
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
            port,
        }
    }
}

/// Renders the paginated library or a watch page; only the selected video receives a media element.
pub async fn home_page_handler(
    State(state): State<AppState>,
    Query(query): Query<HomeQuery>,
) -> Result<impl IntoResponse, TemplateError> {
    let template = {
        let videos = state.index_cache.read().await;
        HomeTemplate::new(&videos, query, state.port)
    };
    Ok(Html(template.render()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::video::Rating;

    fn video(name: &str) -> VideoFile {
        VideoFile {
            file_name: format!("courses/{name}.mp4"),
            display_name: name.to_owned(),
            file_size_mb: 12,
            unix_timestamp: 1,
            format: VideoFormat::Mp4,
            rating: Rating::Unrated,
            tags: vec![Tag::Technology],
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

    #[test]
    fn search_matches_all_terms_across_title_filename_folder_and_tags() {
        let videos = vec![video("Rust Basics"), video("Cooking")];
        let template = HomeTemplate::new(&videos, query("  RUST   courses technology mp4  "), 8081);
        assert_eq!(template.total_count, 1);
        assert_eq!(template.cards[0].title, "Rust Basics");
        assert_eq!(template.search_query, "RUST   courses technology mp4");
        assert_eq!(
            HomeTemplate::new(&videos, query("Rust cooking"), 8081).total_count,
            0
        );
    }

    #[test]
    fn filtering_happens_before_pagination_and_clamps_page() {
        let mut videos: Vec<_> = (0..30).map(|i| video(&format!("Video {i}"))).collect();
        videos.push(video("Unique"));
        let mut search = query("unique");
        search.page = Some(u32::MAX);
        search.tag = Some("TECH".to_owned());
        let template = HomeTemplate::new(&videos, search, 8081);
        assert_eq!(template.current_page, 1);
        assert_eq!(template.total_count, 1);
        let mut search = query("video");
        search.page = Some(2);
        let template = HomeTemplate::new(&videos, search, 8081);
        assert_eq!(template.cards.len(), 6);
        assert!(template.previous_url.contains("search=video"));
    }

    #[test]
    fn feed_escapes_metadata_and_never_embeds_video_players() {
        let videos = vec![video("O'Brien & <script>alert(1)</script>")];
        let html = HomeTemplate::new(&videos, query(""), 8081)
            .render()
            .expect("render feed");
        assert!(!html.contains("<video"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("loading=\"lazy\""));
        assert!(html.contains("data-src=\"/video/courses/a%20%26%20b%2Ejpg\""));
        assert!(html.contains("method=\"get\""));
    }

    #[test]
    fn watch_page_loads_only_selected_video_and_preserves_filters() {
        let videos = vec![video("First"), video("Second")];
        let mut search = query("technology");
        search.tag = Some("Technology".to_owned());
        search.v = Some(videos[1].file_name.clone());
        let template = HomeTemplate::new(&videos, search, 8081);
        assert!(
            template.cards[0]
                .watch_url
                .contains("search=technology&tag=Technology")
        );
        let html = template.render().expect("render watch page");
        assert_eq!(html.matches("<video").count(), 1);
        assert!(html.contains("preload=\"metadata\""));
        assert!(html.contains("src=\"/video/courses/Second%2Emp4\""));
    }

    #[test]
    fn empty_results_and_empty_library_have_distinct_guidance() {
        let empty = HomeTemplate::new(&[], query(""), 8081)
            .render()
            .expect("empty feed");
        assert!(empty.contains("Your library is ready for videos"));
        let filtered = HomeTemplate::new(&[video("First")], query("missing"), 8081)
            .render()
            .expect("empty search");
        assert!(filtered.contains("No videos found"));
        assert!(filtered.contains("Clear filters"));
    }
}
