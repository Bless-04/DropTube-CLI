//! Application startup, background work, and server lifecycle orchestration.

use axum::Router;
use droptube::config::logger::{TuiLogControl, create_log, create_tui_log};
use droptube::models::cli::{self, CliArgs};
use droptube::models::state::AppState;
use droptube::server::{ServerState, TrackingListener, create_router};
use droptube::tui::{self, TuiConfig};
use droptube::utils::display;
use droptube::utils::thumbnails::ThumbnailGenerator;
use local_ip_address::local_ip;
use log::{Level, error, info, warn};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock, oneshot};
use droptube::config::constants::AppExitCode;

const BACKGROUND_SCAN_INTERVAL: Duration = Duration::from_mins(5);

/// Default starting Port for server
const DEFAULT_PORT: u16 = 8081;
struct Application {
    router: Router,
    listener: TcpListener,
    directory: PathBuf,
    local_ip_address: String,
    port: u16,
    tui_log_control: Option<TuiLogControl>,
}

struct BoundListener {
    listener: TcpListener,
    port: u16,
}

struct TuiApplication {
    router: Router,
    listener: TcpListener,
    config: TuiConfig,
    log_control: TuiLogControl,
}

struct ServerRunningGuard {
    running: Arc<AtomicBool>,
}

impl Drop for ServerRunningGuard {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

impl Application {
    async fn initialize() -> Self {
        let args = cli::get();
        let tui_log_control = initialize_logging(args.open_tui);
        install_panic_hook();

        let directory = canonicalize_directory(&args.path);
        let thumbnails = initialize_thumbnails(args).await;
        let mut state = initialize_state(&directory, args.max_depth, thumbnails).await;
        let BoundListener { listener, port } = bind_listener(args.port).await;
        let local_ip_address = discover_local_ip();

        display_startup_summary(&directory, args.max_depth, &local_ip_address, port);
        state.port = port;
        spawn_background_scans(state.clone());

        Self {
            router: create_router(state),
            listener,
            directory,
            local_ip_address,
            port,
            tui_log_control,
        }
    }

    async fn serve(self) {
        let Self {
            router,
            listener,
            directory,
            local_ip_address,
            port,
            tui_log_control,
        } = self;

        if let Some(log_control) = tui_log_control {
            serve_with_tui(TuiApplication {
                listener,
                router,
                config: TuiConfig::new(directory, local_ip_address, port),
                log_control,
            })
            .await;
        } else {
            serve_headless(listener, router).await;
        }
    }
}

/// Initializes and runs DropTube until the active shutdown mechanism completes.
pub(crate) async fn run() {
    Application::initialize().await.serve().await;
}

fn initialize_logging(open_tui: bool) -> Option<TuiLogControl> {
    if open_tui {
        match create_tui_log(Level::Info) {
            Ok(control) => Some(control),
            Err(error) => {
                eprintln!("Failed to attach terminal interface logger: {error}");
                std::process::exit(AppExitCode::Error.into());
            }
        }
    } else {
        if let Err(error) = create_log(Level::Info) {
            eprintln!("Failed to attach logger: {error}");
            std::process::exit(AppExitCode::Error.into());
        }
        None
    }
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown location".to_owned());
        let payload = panic_info.payload();
        let message = if let Some(message) = payload.downcast_ref::<&str>() {
            *message
        } else if let Some(message) = payload.downcast_ref::<String>() {
            message.as_str()
        } else {
            "Box<dyn Any>"
        };

        error!("System panic detected at {location}: {message}");
    }));
}

fn canonicalize_directory(directory: &Path) -> PathBuf {
    match directory.canonicalize() {
        Ok(canonical) => canonical,
        Err(error) => {
            warn!(
                "Could not canonicalize path '{}': {error}. Using as-is.",
                directory.display()
            );
            directory.to_path_buf()
        }
    }
}

async fn initialize_thumbnails(args: &CliArgs) -> Option<ThumbnailGenerator> {
    if !args.thumbnails {
        return None;
    }

    let executable = args
        .ffmpeg_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    match ThumbnailGenerator::new(executable.clone()).await {
        Ok(generator) => Some(generator),
        Err(error) => {
            error!(
                "creating or using thumbnails requires FFmpeg; could not use '{}': {error}. Install FFmpeg on PATH or pass --ffmpeg-path.",
                executable.display()
            );
            std::process::exit(AppExitCode::UsageError.into());
        }
    }
}

async fn initialize_state(
    directory: &Path,
    depth: u8,
    thumbnails: Option<ThumbnailGenerator>,
) -> AppState {
    let state = AppState {
        movie_directory: directory.to_path_buf(),
        port: 0,
        depth,
        index_cache: Arc::new(RwLock::new(Vec::new())),
        thumbnails,
        scan_lock: Arc::new(Mutex::new(())),
    };

    info!("Performing initial filesystem index scan...");
    if let Err(error) = state.refresh_index().await {
        error!("Initial scan failed: {error}");
        std::process::exit(AppExitCode::Error.into());
    }
    state
}

fn discover_local_ip() -> String {
    local_ip()
        .map(|address| address.to_string())
        .unwrap_or_else(|_| "0.0.0.0".to_owned())
}

async fn bind_listener(explicit_port: Option<u16>) -> BoundListener {
    if let Some(port) = explicit_port {
        let bind_address = SocketAddr::from(([0, 0, 0, 0], port));
        return match TcpListener::bind(bind_address).await {
            Ok(listener) => BoundListener { listener, port },
            Err(error) => {
                error!("Failed to bind to port {port}: {error}");
                std::process::exit(AppExitCode::Error.into());
            }
        };
    }

    let mut port = DEFAULT_PORT;
    loop {
        let bind_address = SocketAddr::from(([0, 0, 0, 0], port));
        match TcpListener::bind(bind_address).await {
            Ok(listener) => return BoundListener { listener, port },
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                port = port.saturating_add(1);
            }
            Err(error) => {
                error!("Failed to bind to port {port}: {error}");
                std::process::exit(AppExitCode::Error.into());
            }
        }
    }
}

fn display_startup_summary(directory: &Path, depth: u8, local_ip_address: &str, port: u16) {
    display::title();
    display::serving_dir(directory.display());
    display::scanning_mode(depth);
    let network_url = display::local_urls(local_ip_address, port);
    println!("\x1b[1;36m============================================================\x1b[0m");
    info!("Server startup complete: {network_url}");
}

fn spawn_background_scans(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(BACKGROUND_SCAN_INTERVAL).await;
            if let Err(error) = state.refresh_index().await {
                warn!("Background scan failed: {error}");
            }
        }
    });
}

async fn serve_with_tui(application: TuiApplication) {
    let TuiApplication {
        router,
        listener,
        config,
        log_control,
    } = application;
    let server_state = Arc::new(StdMutex::new(ServerState::default()));
    let tracking_listener = TrackingListener::new(listener, Arc::clone(&server_state));
    let server_running = Arc::new(AtomicBool::new(true));
    let running_after_exit = Arc::clone(&server_running);
    let running_after_signal = Arc::clone(&server_running);
    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let server_task = tokio::spawn(async move {
        let _running_guard = ServerRunningGuard {
            running: running_after_exit,
        };
        axum::serve(tracking_listener, router)
            .with_graceful_shutdown(shutdown_signal(
                Some(shutdown_receiver),
                Some(running_after_signal),
            ))
            .await
    });

    let tui_task = tokio::task::spawn_blocking(move || {
        tui::run_with_logs(server_state, config, server_running, log_control)
    });
    match tui_task.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => error!("Terminal interface failed: {error}"),
        Err(error) => error!("Terminal interface task failed: {error}"),
    }
    if shutdown_sender.send(()).is_err() {
        info!("Server shutdown was already in progress");
    }
    match server_task.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => error!("Server failed: {error}"),
        Err(error) => error!("Server task failed: {error}"),
    }
}

async fn serve_headless(listener: TcpListener, router: Router) {
    if let Err(error) = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal(None, None))
        .await
    {
        error!("Server failed: {error}");
    }
}

async fn shutdown_signal(
    tui_shutdown: Option<oneshot::Receiver<()>>,
    tui_running: Option<Arc<AtomicBool>>,
) {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            error!("Failed to install Ctrl+C handler: {error}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                error!("Failed to install SIGTERM handler: {error}");
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    let tui_requested_shutdown = async {
        if let Some(receiver) = tui_shutdown {
            if receiver.await.is_err() {
                std::future::pending::<()>().await;
            }
        } else {
            std::future::pending::<()>().await;
        }
    };

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C, shutting down..."),
        _ = terminate => info!("Received SIGTERM, shutting down..."),
        _ = tui_requested_shutdown => info!("Terminal interface requested shutdown..."),
    }

    if let Some(running) = tui_running {
        running.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_running_guard_clears_the_flag_when_dropped() {
        let running = Arc::new(AtomicBool::new(true));
        let guard = ServerRunningGuard {
            running: Arc::clone(&running),
        };

        drop(guard);

        assert!(!running.load(Ordering::Acquire));
    }

    #[test]
    fn canonicalize_directory_returns_an_absolute_existing_path() {
        let current_directory = std::env::current_dir()
            .and_then(std::fs::canonicalize)
            .expect("current directory should be canonicalizable");

        let canonical = canonicalize_directory(Path::new("."));

        assert_eq!(canonical, current_directory);
    }

    #[test]
    fn canonicalize_directory_preserves_a_missing_path() {
        let missing =
            std::env::temp_dir().join(format!("droptube-missing-directory-{}", std::process::id()));

        let resolved = canonicalize_directory(&missing);

        assert_eq!(resolved, missing);
    }

    #[tokio::test]
    async fn disabled_thumbnails_do_not_start_ffmpeg() {
        let args = CliArgs {
            max_depth: 0,
            port: None,
            open_tui: false,
            thumbnails: false,
            ffmpeg_path: Some(PathBuf::from("missing-ffmpeg")),
            path: PathBuf::from("."),
        };

        let thumbnails = initialize_thumbnails(&args).await;

        assert!(thumbnails.is_none());
    }

    #[tokio::test]
    async fn explicit_port_binding_uses_the_requested_port() {
        let BoundListener { listener, port } = bind_listener(Some(0)).await;

        assert_eq!(port, 0);
        assert!(listener.local_addr().is_ok());
    }
}
