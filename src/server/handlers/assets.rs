use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;
#[derive(RustEmbed)]
#[folder = "public/"]
struct Assets;
