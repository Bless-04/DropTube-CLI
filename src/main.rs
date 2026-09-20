//! DropTube command-line media server.
#![forbid(unsafe_code)]

use std::process::ExitCode;

mod application;

/// Starts DropTube and runs it until a shutdown signal is received.
#[tokio::main]
async fn main() -> ExitCode {
    application::run().await;
    ExitCode::SUCCESS
}
