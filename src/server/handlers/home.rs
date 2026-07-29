use crate::config::constants::HTML_SOURCE;
use crate::models::state::{AppState, HomeQuery};
use crate::models::video::{Tag, VideoFormat};
use crate::utils::tailwind;
use axum::{
    extract::{Query, State},
    response::Html,
};
use local_ip_address::local_ip;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

/// Handles homepage requests. Lists video files in the served directory.
/// Renders a dynamic player if the query param `v` is set.
pub async fn home_page_handler(
    State(state): State<AppState>,
    Query(query): Query<HomeQuery>,
) -> Html<String> {
    let port = state.port;

    // getting read lock on the cached index immediately
    let videos = {
        let reader = state.index_cache.read().await;
        reader.clone()
    };
    // Gather unique tags for the header filters bubble list
    let mut unique_tags = std::collections::HashSet::new();
    for video in &videos {
        for tag in &video.tags {
            unique_tags.insert(tag.clone());
        }
    }
    let mut unique_tags_list: Vec<Tag> = unique_tags.into_iter().collect();
    unique_tags_list.sort_by_key(|t| t.as_str().to_lowercase());

    let mut tag_filters_html = String::new();
    for tag in unique_tags_list {
        tag_filters_html.push_str(&format!(
            r#"
            <button
                onclick="filterByTag('{}', this)"
                class="tag-btn bg-zinc-800 hover:bg-zinc-700 text-zinc-300 font-normal text-xs px-4 py-1.5 rounded-full active:scale-95 transition-all flex-shrink-0"
            >
                {}
            </button>
            "#,
            tag.as_str(), tag.as_str()
        ));
    }

    // Handle Active Video Details
    let active_video = query
        .v
        .and_then(|v_name| videos.iter().find(|v| v.file_name == v_name).cloned());

    let has_active = active_video.is_some();

    // Render Pinned Video Player (HTML5 video tag)
    let player_html = if let Some(ref video) = active_video {
        let encoded_filename = utf8_percent_encode(&video.file_name, NON_ALPHANUMERIC).to_string();
        let file_size_str = if video.file_size_mb >= 1024 {
            format!("{:.2} GB", (video.file_size_mb as f64) / 1024.0)
        } else {
            format!("{} MB", video.file_size_mb)
        };

        let rating_stars = video.rating.as_stars();
        let rating_color = if video.rating.is_rated() {
            "text-amber-400"
        } else {
            "text-zinc-500"
        };

        let mkv_warning = if video.format == VideoFormat::Mkv {
            r#"
            <div class="mt-3 bg-amber-500 bg-opacity-10 border border-amber-500/30 rounded-lg p-3 text-xs text-amber-400">
                <span class="font-bold">⚠️ MKV Playback Note:</span> Most mobile browsers do not natively support MKV formats. If playback fails to start, we recommend playing it via VLC Player or converting the container to MP4 (H.264).
            </div>
            "#
        } else {
            ""
        };

        // Render tags badges for active video
        let mut active_tags_badges = String::new();
        for tag in &video.tags {
            active_tags_badges.push_str(&format!(
                r#"<span class="px-2 py-0.5 rounded text-[10px] uppercase font-bold tracking-wide {}">{}</span>"#,
                tag.tailwind_badge_class(), tag.as_str()
            ));
        }

        format!(
            r#"
            <div class="lg:col-span-2 flex flex-col">
                <!-- Video Container with 16:9 Aspect Ratio -->
                <div class="relative w-full aspect-video bg-black rounded-none md:rounded-2xl overflow-hidden shadow-2xl border border-zinc-800">
                    <video
                        id="video-player"
                        src="/video/{}"
                        class="w-full h-full"
                        controls
                        autoplay
                        playsinline>
                    </video>
                </div>
                <!-- Video Metadata -->
                <div class="p-4 md:px-0">
                    <div class="flex items-center gap-2 mb-1.5 flex-wrap">
                        {}
                    </div>
                    <h1 class="text-white text-lg md:text-2xl font-bold tracking-tight leading-tight">{}</h1>
                    <div class="mt-2 flex flex-wrap items-center justify-between gap-2 text-xs md:text-sm text-zinc-400 border-b border-zinc-800 pb-4">
                        <div class="flex items-center gap-3">
                            <span class="bg-red-600/15 text-red-500 font-semibold px-2.5 py-0.5 rounded-full text-xs uppercase">{} format</span>
                            <span>•</span>
                            <span>{}</span>
                            <span>•</span>
                            <span class="{} font-mono font-medium tracking-wider">{}</span>
                        </div>
                        <a href="/video/{}" download class="inline-flex items-center gap-1.5 bg-zinc-800 hover:bg-zinc-700 active:scale-95 text-white font-medium px-4 py-1.5 rounded-full transition-all text-xs">
                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/></svg>
                            Download File
                        </a>
                    </div>
                    {}
                </div>
            </div>
            "#,
            encoded_filename,
            active_tags_badges,
            video.display_name,
            video.format.as_str(),
            file_size_str,
            rating_color,
            rating_stars,
            encoded_filename,
            mkv_warning
        )
    } else {
        "".to_string()
    };

    // Render Video Cards
    let mut video_cards_html = String::new();
    for video in &videos {
        let encoded_filename = utf8_percent_encode(&video.file_name, NON_ALPHANUMERIC).to_string();
        let is_playing_card = active_video
            .as_ref()
            .map(|v| v.file_name == video.file_name)
            .unwrap_or(false);
        let active_card_border = if is_playing_card {
            "border border-red-600 ring-2 ring-red-600/20"
        } else {
            "border border-transparent"
        };

        let file_size_str = if video.file_size_mb >= 1024 {
            format!("{:.1} GB", (video.file_size_mb as f64) / 1024.0)
        } else {
            format!("{} MB", video.file_size_mb)
        };

        // Render card tags
        let mut card_tags_html = String::new();
        for tag in &video.tags {
            card_tags_html.push_str(&format!(
                r#"<span class="px-1.5 py-0.2 rounded text-[9px] uppercase font-bold tracking-wide {}">{}</span>"#,
                tag.tailwind_badge_class(), tag.as_str()
            ));
        }

        // Comma-separated tag list for JS filter matching
        let tags_csv = video
            .tags
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<&str>>()
            .join(",");

        let rating_stars = video.rating.as_stars();
        let rating_color = if video.rating.is_rated() {
            "text-amber-400"
        } else {
            "text-zinc-500"
        };

        // Generate thumbnail component (custom image or dynamic gradient fallback)
        let main_thumb_html = if let Some(ref thumb) = video.thumbnail_path {
            let encoded_thumb = utf8_percent_encode(thumb, NON_ALPHANUMERIC).to_string();
            format!(
                r#"<img src="/video/{}" class="w-full h-full object-cover animate-fade-in" alt="Thumbnail" />"#,
                encoded_thumb
            )
        } else {
            format!(
                r#"
                <div class="w-full h-full bg-gradient-to-tr {} flex items-center justify-center">
                    <svg class="w-14 h-14 text-white opacity-80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"/>
                    </svg>
                </div>
                "#,
                tailwind::get_gradient_class(&video.file_name)
            )
        };

        let compact_thumb_html = if let Some(ref thumb) = video.thumbnail_path {
            let encoded_thumb = utf8_percent_encode(thumb, NON_ALPHANUMERIC).to_string();
            format!(
                r#"<img src="/video/{}" class="w-full h-full object-cover animate-fade-in" alt="Thumbnail" />"#,
                encoded_thumb
            )
        } else {
            format!(
                r#"
                <div class="w-full h-full bg-gradient-to-tr {} flex items-center justify-center">
                    <svg class="w-8 h-8 text-white opacity-90 drop-shadow" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"/>
                    </svg>
                </div>
                "#,
                tailwind::get_gradient_class(&video.file_name)
            )
        };

        let card_class = if has_active {
            // Watch page side list (compact horizontal cards)
            format!(
                r#"
                <div class="video-card flex gap-3 p-2 rounded-xl cursor-pointer hover:bg-zinc-900 transition-colors duration-150 {}"
                     data-title="{}"
                     data-tags="{}"
                     onclick="window.location.href='/?v={}'">
                    <!-- Compact Thumbnail -->
                    <div class="relative w-36 aspect-video bg-zinc-800 rounded-lg flex items-center justify-center flex-shrink-0 overflow-hidden shadow-inner">
                        {}
                        <span class="absolute bottom-1 right-1 bg-black/85 text-[10px] px-1 py-0.2 rounded font-semibold text-zinc-100">{}</span>
                    </div>
                    <!-- Details -->
                    <div class="flex flex-col min-w-0 justify-center">
                        <h4 class="text-white text-xs md:text-sm font-semibold truncate leading-tight">{}</h4>
                        <div class="flex items-center gap-1.5 mt-1">
                            <span class="text-[9px] uppercase font-semibold text-zinc-400">{}</span>
                            <span class="{} text-[9px]">{}</span>
                        </div>
                        <div class="flex flex-wrap gap-1 mt-1">
                            {}
                        </div>
                        <span class="time-elapsed text-[10px] text-zinc-500 mt-1" data-timestamp="{}"></span>
                    </div>
                </div>
                "#,
                active_card_border,
                video.display_name,
                tags_csv,
                encoded_filename,
                compact_thumb_html,
                file_size_str,
                video.display_name,
                video.format.as_str(),
                rating_color,
                rating_stars,
                card_tags_html,
                video.unix_timestamp
            )
        } else {
            // Main page feed grid (standard vertical cards)
            format!(
                r#"
                <div class="video-card group flex flex-col bg-[#181818] rounded-2xl cursor-pointer overflow-hidden hover:scale-[1.02] hover:shadow-2xl transition-all duration-200 border border-zinc-800/40"
                     data-title="{}"
                     data-tags="{}"
                     onclick="window.location.href='/?v={}'">
                    <!-- Standard Thumbnail -->
                    <div class="relative w-full aspect-video bg-[#121212] flex items-center justify-center overflow-hidden">
                        <div class="absolute inset-0 bg-black opacity-0 group-hover:opacity-10 transition-opacity duration-200 text-center"></div>
                        {}
                        <div class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity duration-200 text-center">
                            <div class="w-14 h-14 rounded-full bg-black/40 backdrop-blur-sm flex items-center justify-center transform group-hover:scale-110 transition-transform duration-200">
                                <svg class="w-8 h-8 text-white translate-x-0.5" fill="currentColor" viewBox="0 0 24 24">
                                    <path d="M8 5v14l11-7z"/>
                                </svg>
                            </div>
                        </div>
                        <span class="absolute bottom-2 right-2 bg-black/85 text-xs px-2 py-0.5 rounded-md font-semibold text-zinc-100 shadow-md">{}</span>
                    </div>
                    <!-- Details -->
                    <div class="flex p-4 gap-3 bg-[#0f0f0f] border-t border-zinc-900">
                        <div class="w-9 h-9 rounded-full bg-gradient-to-tr from-red-650 to-rose-500 flex-shrink-0 flex items-center justify-center font-bold text-white shadow-md text-sm">🦀</div>
                        <div class="flex flex-col min-w-0 w-full">
                            <h3 class="text-white text-sm font-semibold group-hover:text-red-500 transition-colors line-clamp-2 leading-tight">{}</h3>
                            <div class="flex items-center gap-1.5 mt-1 text-[11px] text-zinc-400 font-medium">
                                <span class="uppercase">{}</span>
                                <span>•</span>
                                <span class="{}">{}</span>
                            </div>
                            <div class="flex flex-wrap gap-1 mt-1.5">
                                {}
                            </div>
                            <span class="time-elapsed text-[10px] text-zinc-500 mt-2 font-medium" data-timestamp="{}"></span>
                        </div>
                    </div>
                </div>
                "#,
                video.display_name,
                tags_csv,
                encoded_filename,
                main_thumb_html,
                file_size_str,
                video.display_name,
                video.format.as_str(),
                rating_color,
                rating_stars,
                card_tags_html,
                video.unix_timestamp
            )
        };
        video_cards_html.push_str(&card_class);
    }

    if video_cards_html.is_empty() {
        video_cards_html = r#"
            <div class="col-span-full flex flex-col items-center justify-center py-24 px-4 text-center">
                <div class="w-16 h-16 rounded-full bg-zinc-800/40 flex items-center justify-center mb-4">
                    <svg class="w-8 h-8 text-zinc-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"/></svg>
                </div>
                <h3 class="text-zinc-300 font-bold text-lg">No Videos Available</h3>
                <p class="text-zinc-500 text-sm mt-1 max-w-xs">Drop mp4, webm or mkv files in the served directory to instantly view them here.</p>
            </div>
            "#.to_string();
    }

    // Dynamic grid container layout logic
    let main_content_html = if has_active {
        format!(
            r#"
            <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 max-w-7xl mx-auto py-0 md:py-6">
                {}
                <div class="lg:col-span-1 p-4 md:p-0 flex flex-col gap-4">
                    <h3 class="text-white text-base font-bold tracking-tight border-b border-zinc-800 pb-2">Up Next</h3>
                    <div class="flex flex-col gap-2 overflow-y-auto max-h-[600px] pr-1 scrollbar-thin">
                        {}
                    </div>
                </div>
            </div>
            "#,
            player_html, video_cards_html
        )
    } else {
        format!(
            r#"
            <div class="max-w-7xl mx-auto px-4 py-8">
                <h2 class="text-white text-lg md:text-xl font-bold tracking-tight mb-6 flex items-center gap-2">
                    <span class="w-1.5 h-6 bg-red-600 rounded-full"></span>
                    Local Video Feed
                </h2>
                <div class="grid grid-cols-1 sm::grid-cols-2 md::grid-cols-3 lg:grid-cols-4 gap-6">
                    {}
                </div>
            </div>
            "#,
            video_cards_html
        )
    };

    let local_ip_addr = local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "0.0.0.0".to_string());

    // Replace template variables
    let full_html = HTML_SOURCE
        .replace("{{LOCAL_IP}}", &local_ip_addr)
        .replace("{{PORT}}", &port.to_string())
        .replace("{{TAG_FILTERS}}", &tag_filters_html)
        .replace("{{CONTENT}}", &main_content_html);

    Html(full_html)
}
