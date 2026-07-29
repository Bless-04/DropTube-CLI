use axum::Router;
use clap::Parser;
use droptube::config::constants::DEFAULT_PORT;
use droptube::config::logger::create_log;
use droptube::models::cli::{CliArgs, CliFlag};
use droptube::models::state::AppState;
use droptube::server::create_router;
use droptube::utils::display;
use droptube::utils::scanner::scan_directory;
use local_ip_address::local_ip;
use log::{Level, error, info, warn};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .unwrap_or_else(|e| error!("{}", e));
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .unwrap_or_else(|e| error!("{}", e))
            .recv()
            .await;
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
    // panic hook for global log safety
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

    // 2. CLI Argument Parsing
    let args: Vec<String> = std::env::args().collect();
    //let args = CliArgs::parse();
    let parsed_flags = match CliFlag::parse_args(&args) {
        Ok(f) => f,
        Err(e) => {
            error!("CLI Error: {}", e);
            eprintln!("Usage: droptube.exe [DIRECTORY] [--port PORT] [--no-recurse]");
            std::process::exit(1);
        }
    };

    create_log(Level::Info).expect("Failed To Attach Logger");

    let mut movie_directory = PathBuf::from(".");
    let mut recurse = true;
    let mut explicit_port = None;

    for flag in parsed_flags {
        match flag {
            CliFlag::NoRecurse => recurse = false,
            CliFlag::Port(p) => explicit_port = Some(p),
            CliFlag::Path(path) => movie_directory = path,
        }
    }

    // Validate directory
    if !movie_directory.exists() {
        error!("Directory '{}' does not exist.", movie_directory.display());
        std::process::exit(1);
    }
    if !movie_directory.is_dir() {
        error!("'{}' is not a directory.", movie_directory.display());
        std::process::exit(1);
    }

    let canonical_dir = movie_directory
        .canonicalize()
        .unwrap_or(movie_directory.clone());

    // 3. Build initial index cache synchronously before server starts to avoid blank page
    println!("\x1b[1;33m[INFO]\x1b[0m Performing initial filesystem index scan...");
    let start_time = SystemTime::now();
    let mut initial_videos = Vec::new();
    let mut count = 0;
    scan_directory(
        &canonical_dir,
        &canonical_dir,
        recurse,
        &mut initial_videos,
        &mut count,
    );
    initial_videos.sort_by_key(|v| std::cmp::Reverse(v.unix_timestamp));

    let duration = start_time.elapsed().map(|d| d.as_millis()).unwrap_or(0);
    print!("\r\x1b[2K"); // Clear the live progress text
    let _ = std::io::Write::flush(&mut std::io::stdout());
    info!(
        "Finished initial scan in {}ms. Found {} video(s) out of {} scanned item(s).",
        duration,
        initial_videos.len(),
        count
    );

    let index_cache = Arc::new(RwLock::new(initial_videos));

    // Discover LAN IP Address
    let local_ip_addr = local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "0.0.0.0".to_string());

    // Bind logic
    let mut port = explicit_port.unwrap_or(DEFAULT_PORT);
    let listener = if explicit_port.is_some() {
        // Strict bind
        let bind_addr = SocketAddr::from(([0, 0, 0, 0], port));
        match tokio::net::TcpListener::bind(bind_addr).await {
            Ok(listener) => listener,
            Err(e) => {
                error!("Failed to bind to port {}: {}", port, e);
                std::process::exit(1);
            }
        }
    } else {
        // Dynamic fallback scan starting from default_port
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
                        active_port += 1;
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
    display::scanning_mode(recurse);

    display::local_urls(local_ip_addr, port);
    println!("\x1b[1;36m============================================================\x1b[0m");

    // Spawning background worker task to re-scan the directory in background
    let cache_clone = index_cache.clone();
    let dir_clone = canonical_dir.clone();
    let recurse_clone = recurse;
    tokio::spawn(async move {
        loop {
            // Re-scan every 30 seconds
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            let dir = dir_clone.clone();
            let scanned = tokio::task::spawn_blocking(move || {
                let mut list = Vec::new();
                let mut count = 0;
                scan_directory(&dir, &dir, recurse_clone, &mut list, &mut count);
                list
            })
            .await
            .unwrap_or_default();

            let mut sorted = scanned;
            sorted.sort_by_key(|v| std::cmp::Reverse(v.unix_timestamp));

            {
                let mut cache_writer = cache_clone.write().await;
                *cache_writer = sorted;
            }
        }
    });

    // 4. Build application routes with panic mitigation middleware
    let app: Router = create_router(AppState {
        movie_directory: canonical_dir.clone(),
        port,
        recurse,
        index_cache,
    });

    // 5. Run the Axum Server with Graceful Shutdown
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        error!("Server failed: {}", e);
    }
}
