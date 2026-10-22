//! Terminal dashboard layout and component rendering.

use super::profiles::ProfilesStatus;
use super::runtime::ViewState;
use crate::server::ClientSnapshot;
use crate::utils;
use qr2term::render::{QrDark, QrLight};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap};
use std::path::PathBuf;
use tui_logger::{ExtLogRecord, LogFormatter, TuiLoggerWidget, TuiWidgetState};

const MENU_WIDTH: u16 = 18;
const MIN_DASHBOARD_WIDTH: u16 = 70;
const MIN_DASHBOARD_HEIGHT: u16 = 14;
const QR_QUIET_ZONE: usize = 4;

/// Page currently visible in the terminal dashboard.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Page {
    /// Server overview and observed clients.
    #[default]
    Dashboard,
    /// Application log records.
    Logs,
    /// Scannable LAN URL.
    QrCode,
    /// Read-only counts from committed profiles.
    Profiles,
}

impl Page {
    /// Returns the following page, wrapping at the end of the menu.
    #[must_use]
    pub(super) const fn next(self) -> Self {
        match self {
            Self::Dashboard => Self::Logs,
            Self::Logs => Self::QrCode,
            Self::QrCode => Self::Profiles,
            Self::Profiles => Self::Dashboard,
        }
    }

    /// Returns the preceding page, wrapping at the start of the menu.
    #[must_use]
    pub(super) const fn previous(self) -> Self {
        match self {
            Self::Dashboard => Self::Profiles,
            Self::Logs => Self::Dashboard,
            Self::QrCode => Self::Logs,
            Self::Profiles => Self::QrCode,
        }
    }
}

/// Immutable details displayed by the terminal interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiConfig {
    directory: PathBuf,
    local_ip_address: String,
    port: u16,
    network_url: String,
}

impl TuiConfig {
    /// Creates terminal display configuration for a bound server.
    #[must_use]
    pub fn new(directory: PathBuf, local_ip_address: String, port: u16) -> Self {
        let network_url = utils::local_url_of(&local_ip_address, port);
        Self {
            directory,
            local_ip_address,
            port,
            network_url,
        }
    }
}

pub(super) fn render(
    frame: &mut ratatui::Frame<'_>,
    config: &TuiConfig,
    clients: &[ClientSnapshot],
    view_state: &ViewState,
    log_state: &TuiWidgetState,
) {
    if frame.area().width < MIN_DASHBOARD_WIDTH || frame.area().height < MIN_DASHBOARD_HEIGHT {
        let message = Paragraph::new(vec![
            Line::from("Terminal too small for the DropTube dashboard"),
            Line::from(format!(
                "Resize to at least {MIN_DASHBOARD_WIDTH}×{MIN_DASHBOARD_HEIGHT}"
            )),
            Line::from("Press q to shut down"),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        frame.render_widget(message, frame.area());
        return;
    }

    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_header(frame, config, header_area);
    let [menu_area, content_area] =
        Layout::horizontal([Constraint::Length(MENU_WIDTH), Constraint::Min(20)]).areas(body_area);
    render_menu(frame, view_state.page, menu_area);
    match view_state.page {
        Page::Dashboard => render_dashboard(
            frame,
            config,
            clients,
            view_state.client_scroll,
            content_area,
        ),
        Page::Logs => render_logs(frame, log_state, content_area),
        Page::QrCode => render_qr_code(frame, config, content_area),
        Page::Profiles => render_profiles(frame, view_state, content_area),
    }

    let footer =
        Paragraph::new("↑/↓/Tab: pages · PgUp/PgDn/Home/End: scroll · 1–4: open · q: shut down")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, footer_area);
}

fn render_header(frame: &mut ratatui::Frame<'_>, config: &TuiConfig, area: Rect) {
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " DropTube ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("● ONLINE", Style::default().fg(Color::Green)),
        Span::raw("    "),
        Span::styled(
            &config.network_url,
            Style::default().fg(Color::LightMagenta),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, area);
}

fn render_menu(frame: &mut ratatui::Frame<'_>, selected: Page, area: Rect) {
    let entries = [
        (Page::Dashboard, "1  Dashboard"),
        (Page::Logs, "2  Logs"),
        (Page::QrCode, "3  QR Code"),
        (Page::Profiles, "4  Profiles"),
    ];
    let items = entries.into_iter().map(|(page, label)| {
        let (prefix, style) = if page == selected {
            (
                "▶ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("  ", Style::default().fg(Color::Gray))
        };
        ListItem::new(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(label, style),
        ]))
    });
    frame.render_widget(
        List::new(items).block(Block::default().title(" Menu ").borders(Borders::ALL)),
        area,
    );
}

fn render_dashboard(
    frame: &mut ratatui::Frame<'_>,
    config: &TuiConfig,
    clients: &[ClientSnapshot],
    client_scroll: usize,
    area: Rect,
) {
    let [summary_area, clients_area] =
        Layout::vertical([Constraint::Length(7), Constraint::Min(3)]).areas(area);
    let active_clients = clients.iter().filter(|client| client.is_active()).count();
    let active_connections = clients
        .iter()
        .map(|client| client.active_connections())
        .sum::<usize>();

    let directory = config.directory.display().to_string();
    let summary = Paragraph::new(vec![
        Line::from(vec![
            metric("ACTIVE CLIENTS", active_clients, Color::Green),
            Span::raw("    "),
            metric("CONNECTIONS", active_connections, Color::Cyan),
            Span::raw("    "),
            metric("OBSERVED", clients.len(), Color::Yellow),
        ]),
        Line::raw(""),
        detail_line("Network", &config.network_url),
        detail_line("Directory", &directory),
    ])
    .block(Block::default().title(" Dashboard ").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, summary_area);

    let available_rows = clients_area.height.saturating_sub(2) as usize;
    let max_start = clients.len().saturating_sub(available_rows);
    let first_visible = client_scroll.min(max_start);
    let visible_count = clients
        .len()
        .saturating_sub(first_visible)
        .min(available_rows);
    let client_items = if clients.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No clients observed yet",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        clients
            .iter()
            .skip(first_visible)
            .take(available_rows)
            .map(|client| {
                let (_, color) = client_status(client.is_active());
                let status = if client.is_active() { "UP" } else { "DOWN" };
                ListItem::new(Line::from(vec![
                    Span::styled("● ", Style::default().fg(color)),
                    Span::styled(format!("{status:<5}"), Style::default().fg(color)),
                    Span::styled(client.ip_address().to_string(), Style::default().fg(color)),
                    Span::raw(format!("  {} conn", client.active_connections())),
                ]))
            })
            .collect()
    };
    let client_title = if clients.is_empty() {
        " Clients observed (0) ".to_owned()
    } else {
        format!(
            " Clients observed ({}) · showing {}-{} ",
            clients.len(),
            first_visible.saturating_add(1),
            first_visible.saturating_add(visible_count)
        )
    };
    frame.render_widget(
        List::new(client_items).block(Block::default().title(client_title).borders(Borders::ALL)),
        clients_area,
    );
}

fn metric(label: &str, value: usize, color: Color) -> Span<'static> {
    Span::styled(
        format!("{label}: {value}"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn render_profiles(frame: &mut ratatui::Frame<'_>, view_state: &ViewState, area: Rect) {
    let [description_area, table_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).areas(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                "Read only · refreshes every 5s",
                Style::default().fg(Color::Cyan),
            ),
            Line::raw("Indexed videos only · edit in the browser"),
            Line::raw("Tags = unique labels used by each profile"),
        ]),
        description_area,
    );
    let profiles = match &view_state.profiles {
        ProfilesStatus::Ready(profiles) if !profiles.is_empty() => profiles,
        status => {
            let message = match status {
                ProfilesStatus::Disconnected => {
                    "Profile source unavailable for this terminal session"
                }
                ProfilesStatus::Loading => "Loading profiles…",
                ProfilesStatus::Ready(_) => {
                    "No saved profiles yet. Rate or tag a video in the browser."
                }
                ProfilesStatus::Error => {
                    "Profiles unavailable. Check library metadata; retrying automatically."
                }
            };
            frame.render_widget(
                Paragraph::new(message)
                    .block(
                        Block::default()
                            .title(" Profiles · read only ")
                            .borders(Borders::ALL),
                    )
                    .wrap(Wrap { trim: true }),
                table_area,
            );
            return;
        }
    };
    let available_rows = usize::from(table_area.height.saturating_sub(3));
    let first_visible = view_state
        .profile_scroll
        .min(profiles.len().saturating_sub(available_rows));
    let visible_count = profiles
        .len()
        .saturating_sub(first_visible)
        .min(available_rows);
    let rows = profiles
        .iter()
        .skip(first_visible)
        .take(available_rows)
        .map(|profile| {
            Row::new(vec![
                Cell::from(profile.name.as_str()),
                Cell::from(profile.rating_count.to_string()),
                Cell::from(profile.tagged_video_count.to_string()),
                Cell::from(profile.tag_count.to_string()),
            ])
        });
    let title = format!(
        " Profiles ({}) · {}–{} · read only ",
        profiles.len(),
        first_visible.saturating_add(1),
        first_visible.saturating_add(visible_count),
    );
    let table = Table::new(
        rows,
        [
            Constraint::Min(10),
            Constraint::Length(6),
            Constraint::Length(13),
            Constraint::Length(5),
        ],
    )
    .header(
        Row::new(["Profile", "Rated", "Tagged videos", "Tags"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .column_spacing(1)
    .block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(table, table_area);
}

fn detail_line<'a>(label: &'a str, value: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{label:<10}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value),
    ])
}

/// Fixed-width column layout for log records.
///
/// Each line renders as:
/// HH:MM:SS | LEVEL | target-name    | message text
///
/// The target column is capped at [LOG_TARGET_WIDTH] characters and the
/// remaining width is given to the message, which wraps onto continuation
/// lines indented to align with the message column.
struct ColumnarLogFormatter;

/// Maximum characters reserved for the log target in the table column.
const LOG_TARGET_WIDTH: usize = 26;

/// Characters before the message column: time(8) + sep(3) + level(5) + sep(3) + target(26) + sep(3).
const LOG_PREFIX_WIDTH: usize = 8 + 3 + 5 + 3 + LOG_TARGET_WIDTH + 3;

impl LogFormatter for ColumnarLogFormatter {
    fn min_width(&self) -> u16 {
        (LOG_PREFIX_WIDTH + 8) as u16
    }

    fn format(&self, width: usize, evt: &ExtLogRecord) -> Vec<Line<'_>> {
        let (level_str, level_color) = match evt.level {
            log::Level::Error => ("ERROR", Color::Red),
            log::Level::Warn  => ("WARN ", Color::Yellow),
            log::Level::Info  => ("INFO ", Color::Cyan),
            log::Level::Debug => ("DEBUG", Color::Blue),
            log::Level::Trace => ("TRACE", Color::DarkGray),
        };
        let meta_style  = Style::default().fg(Color::DarkGray);
        let level_style = Style::default().fg(level_color).add_modifier(Modifier::BOLD);
        let sep_style   = Style::default().fg(Color::DarkGray);
        let msg_style   = Style::default().fg(level_color);
        let timestamp   = evt.timestamp.strftime("%H:%M:%S").to_string();

        // Pad or truncate target to exactly LOG_TARGET_WIDTH chars.
        let raw_target = evt.target();
        let target: String = if raw_target.chars().count() > LOG_TARGET_WIDTH {
            // Keep the rightmost characters, prefix with ellipsis.
            let skip = raw_target.chars().count() - (LOG_TARGET_WIDTH - 1);
            let mut chars = raw_target.chars();
            for _ in 0..skip {
                chars.next();
            }
            format!("{}{}", '\u{2026}', chars.as_str())
        } else {
            format!("{:<width$}", raw_target, width = LOG_TARGET_WIDTH)
        };

        let sep = Span::styled(" \u{2502} ", sep_style);
        let msg_col_width = width.saturating_sub(LOG_PREFIX_WIDTH);

        let mut lines: Vec<Line> = Vec::new();
        let mut first = true;
        for source_line in evt.msg().lines() {
            let chars: Vec<char> = source_line.chars().collect();
            let mut pos = 0usize;
            loop {
                let chunk_end = (pos + msg_col_width.max(1)).min(chars.len());
                let chunk: String = chars[pos..chunk_end].iter().collect();
                if first {
                    first = false;
                    lines.push(Line::from(vec![
                        Span::styled(timestamp.clone(), meta_style),
                        sep.clone(),
                        Span::styled(level_str, level_style),
                        sep.clone(),
                        Span::styled(target.clone(), meta_style),
                        sep.clone(),
                        Span::styled(chunk, msg_style),
                    ]));
                } else {
                    let indent = " ".repeat(LOG_PREFIX_WIDTH);
                    lines.push(Line::from(vec![
                        Span::styled(indent, meta_style),
                        Span::styled(chunk, msg_style),
                    ]));
                }
                pos = chunk_end;
                if pos >= chars.len() {
                    break;
                }
            }
        }

        if lines.is_empty() {
            // Guard against an empty message string.
            lines.push(Line::from(vec![
                Span::styled(timestamp, meta_style),
                sep.clone(),
                Span::styled(level_str, level_style),
                sep.clone(),
                Span::styled(target, meta_style),
                sep,
            ]));
        }

        lines
    }
}

fn render_logs(frame: &mut ratatui::Frame<'_>, log_state: &TuiWidgetState, area: Rect) {
    let widget = TuiLoggerWidget::default()
        .block(
            Block::default()
                .title(" Logs  (time \u{2502} level \u{2502} target \u{2502} message) ")
                .title_style(Style::default().fg(Color::DarkGray))
                .borders(Borders::ALL),
        )
        .formatter(Box::new(ColumnarLogFormatter))
        .style_error(Style::default().fg(Color::Red))
        .style_warn(Style::default().fg(Color::Yellow))
        .style_info(Style::default().fg(Color::Cyan))
        .style_debug(Style::default().fg(Color::Blue))
        .style_trace(Style::default().fg(Color::DarkGray))
        .state(log_state);
    frame.render_widget(widget, area);
}

fn render_qr_code(frame: &mut ratatui::Frame<'_>, config: &TuiConfig, area: Rect) {
    let Ok(mut matrix) =
        qr2term::qr::Qr::from(config.network_url.as_bytes()).map(|qr| qr.to_matrix())
    else {
        frame.render_widget(
            Paragraph::new("Unable to generate QR code").alignment(Alignment::Center),
            area,
        );
        return;
    };
    matrix.surround(QR_QUIET_ZONE, QrLight);
    let qr_width = matrix.size() as u16;
    let qr_height = matrix.size().div_ceil(2) as u16;
    let show_url_below = area.height >= qr_height.saturating_add(2);
    let required_height = if show_url_below {
        qr_height.saturating_add(2)
    } else {
        qr_height
    };
    if area.width < qr_width || area.height < required_height {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("Terminal too small for QR code"),
                Line::from(config.network_url.as_str()),
            ])
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let qr_area = Rect::new(
        area.x + (area.width - qr_width) / 2,
        area.y + (area.height - required_height) / 2,
        qr_width,
        qr_height,
    );
    frame.render_widget(Paragraph::new(qr_lines(&matrix)), qr_area);
    if show_url_below {
        let url_area = Rect::new(qr_area.x, qr_area.y + qr_height + 1, qr_width, 1);
        frame.render_widget(
            Paragraph::new(config.network_url.as_str())
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::LightMagenta)),
            url_area,
        );
    }
}

fn qr_lines(matrix: &qr2term::matrix::Matrix<qr2term::render::Color>) -> Vec<Line<'static>> {
    let size = matrix.size();
    let pixels = matrix.pixels();
    (0..size)
        .step_by(2)
        .map(|row| {
            let spans = (0..size)
                .map(|column| {
                    let top = pixels[row * size + column];
                    let bottom = if row + 1 < size {
                        pixels[(row + 1) * size + column]
                    } else {
                        QrLight
                    };
                    qr_cell(top == QrDark, bottom == QrDark)
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

fn qr_cell(top_dark: bool, bottom_dark: bool) -> Span<'static> {
    match (top_dark, bottom_dark) {
        (true, true) => Span::styled(" ", Style::default().fg(Color::White).bg(Color::Black)),
        (true, false) => Span::styled("▄", Style::default().fg(Color::White).bg(Color::Black)),
        (false, true) => Span::styled("▄", Style::default().fg(Color::Black).bg(Color::White)),
        (false, false) => Span::styled(" ", Style::default().fg(Color::Black).bg(Color::White)),
    }
}

fn client_status(is_active: bool) -> (&'static str, Color) {
    if is_active {
        ("active", Color::Green)
    } else {
        ("disconnected", Color::Red)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn rendered_page(page: Page, width: u16, height: u16) -> String {
        rendered_view(
            ViewState {
                page,
                ..ViewState::default()
            },
            width,
            height,
        )
    }

    fn rendered_view(view_state: ViewState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let config = TuiConfig::new(PathBuf::from("videos"), "192.168.1.2".to_owned(), 8081);

        let log_state = TuiWidgetState::new().set_default_display_level(log::LevelFilter::Info);
        terminal
            .draw(|frame| render(frame, &config, &[], &view_state, &log_state))
            .expect("test dashboard should render");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn pages_cycle_in_both_directions() {
        assert_eq!(
            Page::Dashboard.next(),
            Page::Logs,
            "Next page after Dashboard should be Logs"
        );
        assert_eq!(
            Page::Logs.next(),
            Page::QrCode,
            "Next page after Logs should be QrCode"
        );
        assert_eq!(
            Page::QrCode.next(),
            Page::Profiles,
            "Next page after QrCode should be Profiles"
        );
        assert_eq!(
            Page::Dashboard.previous(),
            Page::Profiles,
            "Previous page before Dashboard should wrap around to Profiles"
        );
        assert_eq!(Page::Profiles.next(), Page::Dashboard);
        assert_eq!(Page::Profiles.previous(), Page::QrCode);
    }

    #[test]
    fn profile_page_reports_loading_empty_errors_and_legacy_disconnection() {
        for (profiles, expected) in [
            (ProfilesStatus::Disconnected, "Profile source unavailable"),
            (ProfilesStatus::Loading, "Loading profiles"),
            (
                ProfilesStatus::Ready(std::sync::Arc::new(Vec::new())),
                "No saved profiles yet",
            ),
            (ProfilesStatus::Error, "Profiles unavailable"),
        ] {
            let rendered = rendered_view(
                ViewState {
                    page: Page::Profiles,
                    profiles,
                    ..ViewState::default()
                },
                100,
                24,
            );
            assert!(rendered.contains(expected), "missing status {expected}");
            assert!(rendered.contains("Read only"));
            assert!(!rendered.contains("No clients observed yet"));
        }
    }

    #[test]
    fn profile_page_displays_counts_and_scrolls_to_the_last_profiles() {
        let profiles = (1..=12)
            .map(|number| crate::profiles::ProfileSummary {
                name: format!("profile{number:02}"),
                rating_count: number,
                tagged_video_count: number + 1,
                tag_count: number + 2,
            })
            .collect::<Vec<_>>();
        let rendered = rendered_view(
            ViewState {
                page: Page::Profiles,
                profile_scroll: usize::MAX,
                profiles: ProfilesStatus::Ready(std::sync::Arc::new(profiles)),
                ..ViewState::default()
            },
            100,
            14,
        );
        assert!(rendered.contains("profile12"));
        assert!(!rendered.contains("profile01"));
        assert!(rendered.contains("Rated"));
        assert!(rendered.contains("Tagged videos"));
        assert!(rendered.contains("Tags"));
        assert!(rendered.contains("12"));
        assert!(rendered.contains("13"));
        assert!(rendered.contains("14"));
    }

    #[test]
    fn dashboard_renders_menu_server_and_empty_client_details() {
        let rendered = rendered_page(Page::Dashboard, 100, 24);
        assert!(
            rendered.contains("Dashboard"),
            "Dashboard page should render Dashboard header title"
        );
        assert!(
            rendered.contains("http://192.168.1.2:8081"),
            "Dashboard page should render server LAN URL"
        );
        assert!(
            rendered.contains("No clients observed yet"),
            "Dashboard page should show empty state when no clients have connected"
        );
    }

    #[test]
    fn logs_have_a_dedicated_page() {
        let rendered = rendered_page(Page::Logs, 100, 24);
        assert!(
            rendered.contains("Logs"),
            "Logs page should render Logs tab title"
        );
        assert!(
            !rendered.contains("No clients observed yet"),
            "Logs page should not render dashboard client panel"
        );
    }

    #[test]
    fn logs_page_renders_records_from_tui_logger() {
        let drain = tui_logger::Drain::new();
        drain.log(
            &log::Record::builder()
                .level(log::Level::Warn)
                .target("droptube::tui_widget_test")
                .args(format_args!("tui-logger-integration-record"))
                .build(),
        );
        tui_logger::move_events();

        let rendered = rendered_page(Page::Logs, 100, 24);

        assert!(
            rendered.contains("WARN"),
            "Logs page should render log severity level WARN"
        );
        assert!(
            rendered.contains("droptube::tui_widget_test"),
            "Logs page should render log target name"
        );
        assert!(
            rendered.contains("tui-logger-integration-record"),
            "Logs page should render formatted log record message"
        );
    }

    #[test]
    fn qr_page_renders_without_ansi_escape_sequences() {
        let rendered = rendered_page(Page::QrCode, 100, 34);
        assert!(
            rendered.contains("http://192.168.1.2:8081"),
            "QR page should display formatted server address URL"
        );
        assert!(
            rendered.contains('▄'),
            "QR page should render Unicode lower half block character for QR matrix"
        );
        assert!(
            !rendered.contains('\u{1b}'),
            "QR page rendering should not contain raw ANSI escape sequences"
        );
    }

    #[test]
    fn qr_page_fits_an_eighty_by_twenty_four_terminal() {
        let rendered = rendered_page(Page::QrCode, 80, 24);
        assert!(
            rendered.contains('▄'),
            "QR code should fit and render blocks on standard 80x24 terminal"
        );
        assert!(
            !rendered.contains("Terminal too small for QR code"),
            "80x24 terminal should not show small-terminal fallback message"
        );
    }

    #[test]
    fn small_qr_page_has_a_readable_fallback() {
        let rendered = rendered_page(Page::QrCode, 70, 16);
        assert!(
            rendered.contains("Terminal too small"),
            "Compact terminal under 80x24 should display 'Terminal too small' guidance"
        );
        assert!(
            rendered.contains("QR code"),
            "Compact terminal fallback guidance should specifically mention QR code"
        );
    }

    #[test]
    fn very_small_terminal_has_a_global_resize_message() {
        let rendered = rendered_page(Page::Dashboard, 50, 8);
        assert!(
            rendered.contains("Terminal too small for the DropTube dashboard"),
            "Terminal smaller than minimum bounds should display global resize warning"
        );
        assert!(
            rendered.contains("70×14"),
            "Resize warning message should state required minimum size 70×14"
        );
    }

    #[test]
    fn active_clients_are_green_and_disconnected_clients_are_red() {
        assert_eq!(
            client_status(true),
            ("active", Color::Green),
            "Active client status should be 'active' formatted in Green"
        );
        assert_eq!(
            client_status(false),
            ("disconnected", Color::Red),
            "Disconnected client status should be 'disconnected' formatted in Red"
        );
    }

    #[test]
    fn client_ip_addresses_use_their_connection_status_color() {
        let backend = TestBackend::new(MIN_DASHBOARD_WIDTH, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let config = TuiConfig::new(PathBuf::from("videos"), "192.168.1.2".to_owned(), 8081);
        let clients = [
            ClientSnapshot::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)), 1),
            ClientSnapshot::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 11)), 0),
            ClientSnapshot::new(
                IpAddr::V6(Ipv6Addr::new(
                    0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff,
                )),
                1,
            ),
        ];
        let view_state = ViewState::default();
        let log_state = TuiWidgetState::new().set_default_display_level(log::LevelFilter::Info);

        terminal
            .draw(|frame| render(frame, &config, &clients, &view_state, &log_state))
            .expect("dashboard should render clients");
        let buffer = terminal.backend().buffer();
        let green_text = buffer
            .content()
            .iter()
            .filter(|cell| cell.fg == Color::Green)
            .map(|cell| cell.symbol())
            .collect::<String>();
        let red_text = buffer
            .content()
            .iter()
            .filter(|cell| cell.fg == Color::Red)
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(
            green_text.contains("192.168.1.10"),
            "Active client IPv4 address should be colored Green"
        );
        assert!(
            green_text.contains("ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
            "Active client IPv6 address should be colored Green"
        );
        assert!(
            red_text.contains("192.168.1.11"),
            "Disconnected client IP address should be colored Red"
        );
    }

    #[test]
    fn client_list_can_render_entries_beyond_the_first_viewport() {
        let backend = TestBackend::new(100, 14);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let config = TuiConfig::new(PathBuf::from("videos"), "192.168.1.2".to_owned(), 8081);
        let clients = (1..=12)
            .map(|last_octet| {
                ClientSnapshot::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, last_octet)), 1)
            })
            .collect::<Vec<_>>();
        let view_state = ViewState {
            client_scroll: usize::MAX,
            ..ViewState::default()
        };
        let log_state = TuiWidgetState::new().set_default_display_level(log::LevelFilter::Info);

        terminal
            .draw(|frame| render(frame, &config, &clients, &view_state, &log_state))
            .expect("scrolled client list should render");
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(
            rendered.contains("192.168.1.12"),
            "Scrolled client viewport should show the last client entry"
        );
        assert!(
            !rendered.contains("192.168.1.1 "),
            "Scrolled client viewport should have scrolled past early client entries"
        );
    }
}
