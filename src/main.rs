use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
    http::StatusCode,
};
use local_ip_address::local_ip;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use ui::tailwind;

pub mod config;
pub mod ui;
pub mod routes;

// Embed the HTML & JS template at compile time
const INDEX_HTML_TEMPLATE: &str = include_str!("../public/index.html");


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
