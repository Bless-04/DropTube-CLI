//! DropTube command-line media server.
#![forbid(unsafe_code)]

mod application;

/// Starts DropTube and runs it until a shutdown signal is received.
#[tokio::main]
async fn main() {
    application::run().await;
}
