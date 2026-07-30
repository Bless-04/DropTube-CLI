use axum::Router;

use droptube::config::constants::DEFAULT_PORT;
use droptube::config::logger::create_log;
use droptube::models::cli;
use droptube::models::state::AppState;
use droptube::server::create_router;
use droptube::utils::display;
use droptube::utils::scanner::{ScanDirectoryParams, scan_directory};
use local_ip_address::local_ip;
use log::{Level, error, info, warn};
use std::cmp::Reverse;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to install Ctrl+C handler: {}", e);
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                error!("Failed to install SIGTERM handler: {}", e);
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down...");
        },
        _ = terminate => {
            info!("Received SIGTERM, shutting down...");
        },
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = create_log(Level::Info) {
        eprintln!("Failed to attach logger: {}", e);
        std::process::exit(1);
    }

    // Global panic hook ; logs panics through the tracing/log stack .
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());

        let payload = panic_info.payload();
        let message = if let Some(s) = payload.downcast_ref::<&str>() {
            *s
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.as_str()
        } else {
            "Box<dyn Any>"
        };

        error!("System panic detected at {}: {}", location, message);
    }));

    // CLI Argument Parsing
    let args = cli::get();
    let movie_directory = args.path.clone();
    let depth = args.max_depth;
    let explicit_port = args.port;

    // Validate directory
    if !movie_directory.exists() {
        error!("Directory '{}' does not exist.", movie_directory.display());
        std::process::exit(1);
    }
    if !movie_directory.is_dir() {
        error!("'{}' is not a directory.", movie_directory.display());
        std::process::exit(1);
    }

    let canonical_dir = match movie_directory.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "Could not canonicalize path '{}': {}. Using as-is.",
                movie_directory.display(),
                e
            );
            movie_directory.clone()
        }
    };

    // Build initial index cache synchronously before the server starts to avoid a blank page.
    info!("Performing initial filesystem index scan...");
    let start_time = SystemTime::now();

    let mut initial_params = ScanDirectoryParams::new(canonical_dir.clone(), depth);
    scan_directory(&mut initial_params);

    let scan_count = initial_params.count;
    let mut initial_videos = initial_params.videos;
    initial_videos.sort_by_key(|v| Reverse(v.unix_timestamp));

    let duration = start_time.elapsed().map(|d| d.as_millis()).unwrap_or(0);
    info!(
        "Finished initial scan in {duration}ms. Found {} video(s) out of {} scanned item(s).",
        initial_videos.len(),
        scan_count
    );

    let index_cache = Arc::new(RwLock::new(initial_videos));

    // Discover LAN IP address for display
    let local_ip_addr = local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "0.0.0.0".to_string());

    // Bind to port (strict if explicit, otherwise scan upward from default)
    let mut port = explicit_port.unwrap_or(DEFAULT_PORT);
    let listener = if explicit_port.is_some() {
        let bind_addr = SocketAddr::from(([0, 0, 0, 0], port));
        match tokio::net::TcpListener::bind(bind_addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind to port {}: {}", port, e);
                std::process::exit(1);
            }
        }
    } else {
        let mut active_port = DEFAULT_PORT;
        loop {
            let bind_addr = SocketAddr::from(([0, 0, 0, 0], active_port));
            match tokio::net::TcpListener::bind(bind_addr).await {
                Ok(l) => {
                    port = active_port;
                    break l;
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::AddrInUse {
                        active_port = active_port.saturating_add(1);
                    } else {
                        error!("Failed to bind to port {}: {}", active_port, e);
                        std::process::exit(1);
                    }
                }
            }
        }
    };

    println!("\x1b[1;36m============================================================\x1b[0m");
    println!("🎬 \x1b[1;32mDropTube\x1b[0m - Local Media Server");
    println!("\x1b[1;36m============================================================\x1b[0m");
    display::serving_dir(canonical_dir.display());
    display::scanning_mode(depth);
    display::local_urls(local_ip_addr, port);
    println!("\x1b[1;36m============================================================\x1b[0m");

    // Background worker: re-scans the directory every 30 seconds.
    let cache_clone = index_cache.clone();
    let dir_clone = canonical_dir.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            let dir = dir_clone.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut params = ScanDirectoryParams::new(dir, depth);
                scan_directory(&mut params);
                params.videos
            })
            .await;

            match result {
                Ok(mut videos) => {
                    videos.sort_by_key(|v| Reverse(v.unix_timestamp));
                    let mut cache_writer = cache_clone.write().await;
                    *cache_writer = videos;
                }
                Err(e) => {
                    warn!("Background scan task panicked: {}", e);
                }
            }
        }
    });

    // Build application routes
    let app: Router = create_router(AppState {
        movie_directory: canonical_dir.clone(),
        port,
        depth,
        index_cache,
    });

    // Run the Axum server with graceful shutdown
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        error!("Server failed: {}", e);
    }
}
