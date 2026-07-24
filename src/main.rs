use axum::Router;
use droptube::config::logger::create_log;
use droptube::models::flags::CliFlag;
use droptube::models::state::AppState;
use droptube::server::create_router;
use droptube::utils::scanner::scan_directory;
use local_ip_address::local_ip;
use log::{Level, error, info};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
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

        }
    };


    let mut movie_directory = PathBuf::from(".");
    let mut recurse = true;
    let mut explicit_port = None;

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
    println!("\x1b[1;36m============================================================\x1b[0m");
    println!("🎬 \x1b[1;32mDropTube\x1b[0m - High-Performance Rust Media Server");
    println!("\x1b[1;36m============================================================\x1b[0m");
    println!("📂 Serving Directory : \x1b[1;34m{}\x1b[0m", canonical_dir.display());
    println!("⚙️  Scanning Mode     : \x1b[1;33m{}\x1b[0m", if recurse { "Recursive" } else { "Immediate Directory Only" });
    println!("🚀 Local Access      : \x1b[1;35mhttp://localhost:{}\x1b[0m", port);
    println!("📱 Mobile Stream LAN : \x1b[1;35mhttp://{}:{}\x1b[0m", local_ip_addr, port);
    println!("\x1b[1;36m============================================================\x1b[0m");

    // Spawning background worker task to re-scan the directory in background
    let cache_clone = index_cache.clone();
    let dir_clone = canonical_dir.clone();
}
