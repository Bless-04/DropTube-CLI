//! Local video indexing, thumbnail generation, and HTTP streaming for DropTube.
#![forbid(unsafe_code)]

/// module for config
pub mod config;

/// module for stateful models
pub mod models;

/// Persistent shared profiles, personal metadata, and derived community ratings.
pub mod profiles;

/// module for axum routing and endpoint handling
pub mod server;

/// Optional terminal user interface.
pub mod tui;

/// module used for macros
pub mod macros;
/// module for functions to make life easier
pub mod utils;
