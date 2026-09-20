//! Terminal dashboard layout and component rendering.

use crate::server::ClientSnapshot;
use crate::utils;
use qr2term::render::{QrDark, QrLight};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use std::path::PathBuf;
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetState};

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
}

impl Page {
    /// Returns the following page, wrapping at the end of the menu.
    #[must_use]
    pub(super) const fn next(self) -> Self {
        match self {
            Self::Dashboard => Self::Logs,
            Self::Logs => Self::QrCode,
            Self::QrCode => Self::Dashboard,
        }
    }

    /// Returns the preceding page, wrapping at the start of the menu.
    #[must_use]
    pub(super) const fn previous(self) -> Self {
        match self {
            Self::Dashboard => Self::QrCode,
            Self::Logs => Self::Dashboard,
            Self::QrCode => Self::Logs,
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
    view_state: &super::ViewState,
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
    }

    let footer = Paragraph::new(
        "↑/↓ or Tab: pages  •  PgUp/PgDn: scroll  •  End: live logs  •  1/2/3: open  •  q: shut down",
    )
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

fn detail_line<'a>(label: &'a str, value: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{label:<10}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value),
    ])
}

fn render_logs(frame: &mut ratatui::Frame<'_>, log_state: &TuiWidgetState, area: Rect) {
    let widget = TuiLoggerWidget::default()
        .block(Block::default().title(" Logs ").borders(Borders::ALL))
        .output_timestamp(Some("%H:%M:%S".to_owned()))
        .output_level(Some(TuiLoggerLevelOutput::Long))
        .output_target(true)
        .output_file(false)
        .output_line(false)
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
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let config = TuiConfig::new(PathBuf::from("videos"), "192.168.1.2".to_owned(), 8081);

        let view_state = super::super::ViewState {
            page,
            ..super::super::ViewState::default()
        };
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
        assert_eq!(Page::Dashboard.next(), Page::Logs);
        assert_eq!(Page::Logs.next(), Page::QrCode);
        assert_eq!(Page::QrCode.next(), Page::Dashboard);
        assert_eq!(Page::Dashboard.previous(), Page::QrCode);
    }

    #[test]
    fn dashboard_renders_menu_server_and_empty_client_details() {
        let rendered = rendered_page(Page::Dashboard, 100, 24);
        assert!(rendered.contains("Dashboard"));
        assert!(rendered.contains("http://192.168.1.2:8081"));
        assert!(rendered.contains("No clients observed yet"));
    }

    #[test]
    fn logs_have_a_dedicated_page() {
        let rendered = rendered_page(Page::Logs, 100, 24);
        assert!(rendered.contains("Logs"));
        assert!(!rendered.contains("No clients observed yet"));
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

        assert!(rendered.contains("WARN"));
        assert!(rendered.contains("droptube::tui_widget_test"));
        assert!(rendered.contains("tui-logger-integration-record"));
    }

    #[test]
    fn qr_page_renders_without_ansi_escape_sequences() {
        let rendered = rendered_page(Page::QrCode, 100, 34);
        assert!(rendered.contains("http://192.168.1.2:8081"));
        assert!(rendered.contains('▄'));
        assert!(!rendered.contains('\u{1b}'));
    }

    #[test]
    fn qr_page_fits_an_eighty_by_twenty_four_terminal() {
        let rendered = rendered_page(Page::QrCode, 80, 24);
        assert!(rendered.contains('▄'));
        assert!(!rendered.contains("Terminal too small for QR code"));
    }

    #[test]
    fn small_qr_page_has_a_readable_fallback() {
        let rendered = rendered_page(Page::QrCode, 70, 16);
        assert!(rendered.contains("Terminal too small"));
        assert!(rendered.contains("QR code"));
    }

    #[test]
    fn very_small_terminal_has_a_global_resize_message() {
        let rendered = rendered_page(Page::Dashboard, 50, 8);
        assert!(rendered.contains("Terminal too small for the DropTube dashboard"));
        assert!(rendered.contains("70×14"));
    }

    #[test]
    fn active_clients_are_green_and_disconnected_clients_are_red() {
        assert_eq!(client_status(true), ("active", Color::Green));
        assert_eq!(client_status(false), ("disconnected", Color::Red));
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
        let view_state = super::super::ViewState::default();
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
        assert!(green_text.contains("192.168.1.10"));
        assert!(green_text.contains("ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"));
        assert!(red_text.contains("192.168.1.11"));
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
        let view_state = super::super::ViewState {
            client_scroll: usize::MAX,
            ..super::super::ViewState::default()
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
        assert!(rendered.contains("192.168.1.12"));
        assert!(!rendered.contains("192.168.1.1 "));
    }
}
