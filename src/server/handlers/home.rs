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
            <div class="col-12 col-lg-8 mb-4">
                <!-- Video wrapper: position:relative so the seek overlay can be placed on top -->
                <div class="ratio ratio-16x9 bg-black rounded overflow-hidden shadow-sm" style="position:relative;">
                    <video
                        id="video-player"
                        src="/video/{}"
                        class="w-100 h-100"
                        controls
                        autoplay
                        playsinline>
                    </video>

                    <!-- Double-tap seek overlay (pointer-events:none in the centre so controls still work) -->
                    <div id="seek-overlay" style="
                        position:absolute; inset:0; display:grid;
                        grid-template-columns:30% 40% 30%;
                        pointer-events:none; z-index:10;">
                        <!-- Left zone: seek back 5 s -->
                        <div id="seek-left" style="pointer-events:auto; cursor:pointer;" aria-label="Seek back 5 seconds"></div>
                        <!-- Centre zone: pass clicks through to the native video controls -->
                        <div style="pointer-events:none;"></div>
                        <!-- Right zone: seek forward 5 s -->
                        <div id="seek-right" style="pointer-events:auto; cursor:pointer;" aria-label="Seek forward 5 seconds"></div>
                    </div>

                    <!-- Seek ripple feedback badge -->
                    <div id="seek-badge" style="
                        position:absolute; top:50%; left:50%; transform:translate(-50%,-50%);
                        background:rgba(0,0,0,0.65); color:#fff; font-size:0.85rem; font-weight:600;
                        padding:6px 14px; border-radius:20px; pointer-events:none;
                        opacity:0; transition:opacity 0.15s ease; white-space:nowrap; z-index:20;">
                    </div>
                </div>
                <div class="p-3">
                    <div class="d-flex flex-wrap gap-1 mb-2">
                        {}
                    </div>
                    <h4 class="fw-bold mb-2">{}</h4>
                    <div class="d-flex flex-wrap align-items-center justify-content-between text-muted small border-bottom pb-3">
                        <div class="d-flex align-items-center gap-2">
                            <span class="badge bg-danger">{} format</span>
                            <span>•</span>
                            <span>{}</span>
                            <span>•</span>
                            <span class="{}">{}</span>
                        </div>
                        <a href="/video/{}" download class="btn btn-dark btn-sm rounded-pill mt-2 mt-sm-0 text-decoration-none">
                            <i class="fas fa-download me-1"></i> Download
                        </a>
                    </div>
                    {}
                </div>
            </div>

            <script>
            (function () {{
                'use strict';

                /** Milliseconds between two taps/clicks that count as a double-tap. */
                var DOUBLE_TAP_MS = 300;
                /** Seconds to seek per double-tap. */
                var SEEK_SECONDS = 5;
                /** How long (ms) the feedback badge stays visible. */
                var BADGE_DURATION_MS = 600;

                var video = document.getElementById('video-player');
                var badge = document.getElementById('seek-badge');
                var badgeTimer = null;

                /**
                 * Shows the seek-feedback badge then fades it out.
                 * @param {{string}} text - Label to display inside the badge.
                 */
                function showBadge(text) {{
                    badge.textContent = text;
                    badge.style.opacity = '1';
                    clearTimeout(badgeTimer);
                    badgeTimer = setTimeout(function () {{
                        badge.style.opacity = '0';
                    }}, BADGE_DURATION_MS);
                }}

                /**
                 * Attaches double-tap/double-click detection to a zone element.
                 * On double interaction, `onDoubleTap` is called.
                 * @param {{HTMLElement}} zone
                 * @param {{function(): void}} onDoubleTap
                 */
                function attachDoubleTap(zone, onDoubleTap) {{
                    var lastTap = 0;

                    // Touch devices
                    zone.addEventListener('touchend', function (e) {{
                        var now = Date.now();
                        if (now - lastTap < DOUBLE_TAP_MS) {{
                            e.preventDefault(); // prevent the browser triggering a synthetic click too
                            onDoubleTap();
                            lastTap = 0;
                        }} else {{
                            lastTap = now;
                        }}
                    }}, {{ passive: false }});

                    // Mouse / desktop
                    zone.addEventListener('dblclick', function (e) {{
                        e.preventDefault();
                        onDoubleTap();
                    }});
                }}

                attachDoubleTap(document.getElementById('seek-left'), function () {{
                    video.currentTime = Math.max(0, video.currentTime - SEEK_SECONDS);
                    showBadge('\u21a9 ' + SEEK_SECONDS + 's');
                }});

                attachDoubleTap(document.getElementById('seek-right'), function () {{
                    video.currentTime = Math.min(video.duration || Infinity, video.currentTime + SEEK_SECONDS);
                    showBadge(SEEK_SECONDS + 's \u21aa');
                }});
            }})();
            </script>
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
        let _active_card_border = if is_playing_card {
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
                <div class="d-flex gap-2 p-2 rounded cursor-pointer {}"
                     data-title="{}"
                     data-tags="{}"
                     onclick="window.location.href='/?v={}'">
                    <div class="position-relative bg-dark rounded flex-shrink-0" style="width: 160px; height: 90px; overflow: hidden;">
                        {}
                        <span class="position-absolute bottom-0 end-0 bg-dark text-white px-1 m-1 rounded" style="font-size: 0.7rem;">{}</span>
                    </div>
                    <div class="d-flex flex-column justify-content-center overflow-hidden w-100">
                        <h6 class="text-truncate mb-1" style="font-size: 0.9rem;">{}</h6>
                        <div class="text-muted" style="font-size: 0.75rem;">
                            <span class="text-uppercase">{}</span>
                            <span class="{}">{}</span>
                        </div>
                        <div class="d-flex flex-wrap gap-1 mt-1">
                            {}
                        </div>
                        <span class="time-elapsed text-muted mt-1" style="font-size: 0.75rem;" data-timestamp="{}"></span>
                    </div>
                </div>
                "#,
                if is_playing_card {
                    "bg-light border border-danger"
                } else {
                    "hover-bg-light"
                },
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
                <div class="col-12 col-sm-6 col-lg-4 col-xl-3 video-card-container">
                    <div class="video-card h-100" data-title="{}" data-tags="{}">
                        <a href="/?v={}" class="text-decoration-none text-dark d-block">
                            <div class="position-relative bg-dark" style="aspect-ratio: 16/9; overflow: hidden;">
                                {}
                                <span class="position-absolute bottom-0 end-0 bg-dark text-white px-1 m-1 rounded" style="font-size: 0.75rem; opacity: 0.85;">{}</span>
                            </div>
                            <div class="card-body p-2 mt-2">
                                <div class="d-flex">
                                    <div class="channel-icon me-2">
                                        <div class="rounded-circle bg-danger text-white d-flex align-items-center justify-content-center" style="width:36px; height:36px; font-size:14px; font-weight:bold;">🦀</div>
                                    </div>
                                    <div class="w-100 overflow-hidden">
                                        <h6 class="card-title mb-1 text-truncate" style="font-size: 0.95rem; font-weight: 600;">{}</h6>
                                        <p class="card-text video-stats mb-0 text-muted" style="font-size: 0.8rem;">
                                            <span class="text-uppercase">{}</span> • 
                                            <span class="{}">{}</span>
                                        </p>
                                        <div class="d-flex flex-wrap gap-1 mt-1">
                                            {}
                                        </div>
                                        <p class="time-elapsed text-muted mt-1 mb-0" style="font-size: 0.75rem;" data-timestamp="{}"></p>
                                    </div>
                                </div>
                            </div>
                        </a>
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
            <div class="col-12 text-center py-5 mt-5">
                <div class="mb-3 text-muted">
                    <i class="fas fa-video fa-3x"></i>
                </div>
                <h3 class="fw-bold">No Videos Available</h3>
                <p class="text-muted">Drop mp4, webm or mkv files in the served directory to instantly view them here.</p>
            </div>
            "#.to_string();
    }

    // Dynamic grid container layout logic
    let main_content_html = if has_active {
        format!(
            r#"
            <div class="row">
                {}
                <div class="col-12 col-lg-4">
                    <h5 class="fw-bold mb-3 border-bottom pb-2">Up Next</h5>
                    <div class="d-flex flex-column gap-2 pe-2" style="max-height: 600px; overflow-y: auto;">
                        {}
                    </div>
                </div>
            </div>
            "#,
            player_html, video_cards_html
        )
    } else {
        // Just return the video cards directly, the template already has `<div class="row g-4">` surrounding `{{CONTENT}}`
        // Wait, if I return it directly, they will be inside the `row g-4`.
        // Let's wrap them in a fragment or just return them as is, because `{{CONTENT}}` is inside the `row g-4` in the template.
        // Wait, what if we are in the `has_active` case? In that case, `player_html` and `video_cards_html` are side by side.
        // Is `{{CONTENT}}` inside `<div class="row g-4">` in our `website/index.html`?
        // Yes! So for `has_active`, the `<div class="row">` inside `<div class="row g-4">` might be weird but acceptable if we use `col-12`.
        // Let's just output `video_cards_html` for the `else` block.
        video_cards_html
    };

    let local_ip_addr = local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "0.0.0.0".to_string());

    // Replace template variables
    let full_html = HTML_SOURCE
        .replace("{{LOCAL_IP}}", &local_ip_addr)
        .replace("{{PORT}}", &port.to_string())
        .replace("{{TAG_FILTERS}}", &tag_filters_html)
        .replace("{{ACTIVE_HOME}}", "active")
        .replace("{{ACTIVE_EXPLORER}}", "")
        .replace("{{CONTENT}}", &main_content_html);

    Html(full_html)
}
