//! DropTube command-line media server.
#![forbid(unsafe_code)]

use axum::Router;

use droptube::config::constants::DEFAULT_PORT;
use droptube::config::logger::create_log;
use droptube::models::cli;
use droptube::models::state::AppState;
use droptube::server::create_router;
use droptube::utils::display;
use droptube::utils::thumbnails::ThumbnailGenerator;
use local_ip_address::local_ip;
use log::{Level, error, info, warn};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Graceful Shutdown Signal Handler
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

/// Entry Point for the application
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
    let src_dir = args.path.clone();
    let explicit_port = args.port;

    // Validate directory
    if !src_dir.exists() {
        error!("Directory '{}' does not exist.", src_dir.display());
        std::process::exit(1);
    }
    if !src_dir.is_dir() {
        error!("'{}' is not a directory.", src_dir.display());
        std::process::exit(1);
    }

    let canonical_dir = match src_dir.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "Could not canonicalize path '{}': {}. Using as-is.",
                src_dir.display(),
                e
            );
            src_dir.clone()
        }
    };

    let thumbnails = if args.thumbnails {
        let executable = args.ffmpeg_path.clone().unwrap_or_else(|| "ffmpeg".into());
        match ThumbnailGenerator::new(executable.clone()).await {
            Ok(generator) => Some(generator),
            // cant enable thumbnails if FFmpeg isnt found. and fails early
            Err(error) => panic!(
                "--thumbnails requires FFmpeg; could not use '{}': {error}. Install FFmpeg on PATH or pass --ffmpeg-path.",
                executable.display()
            ),
        }
    } else {
        None
    };

    let mut state = AppState {
        movie_directory: canonical_dir.clone(),
        port: 0,
        depth: args.max_depth,
        index_cache: Arc::new(RwLock::new(Vec::new())),
        thumbnails,
        scan_lock: Arc::new(Mutex::new(())),
    };
    info!("Performing initial filesystem index scan...");
    if let Err(error) = state.refresh_index().await {
        error!("Initial scan failed: {error}");
        std::process::exit(1);
    }

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
                        // address goes up by 1 if in use
                        active_port = active_port.saturating_add(1);
                    } else {
                        error!("Failed to bind to port {}: {}", active_port, e);
                        std::process::exit(1);
                    }
                }
            }
        }
    };

    display::title();
    display::serving_dir(canonical_dir.display());
    display::scanning_mode(args.max_depth);
    display::local_urls(local_ip_addr, port);
    println!("\x1b[1;36m============================================================\x1b[0m");

    state.port = port;
    let background_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_mins(5)).await; //refresh index every 5 mins ; todo this should be opt in and be controlled by a cli flag called 
            if let Err(error) = background_state.refresh_index().await {
                warn!("Background scan failed: {error}");
            }
        }
    });

    let app: Router = create_router(state);

    // Run the Axum server with graceful shutdown
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        error!("Server failed: {}", e);
    }
}
