//! Local video indexing, thumbnail generation, and HTTP streaming for DropTube.
#![forbid(unsafe_code)]

/// module for config
pub mod config;

/// module for stateful models
pub mod models;

/// module for axum routing and endpoint handling
pub mod server;

/// module for functions to make life easier
pub mod utils;
