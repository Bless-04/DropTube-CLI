use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "public/"]
struct Assets;

pub async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content.data))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
            .into_response()
    } else {
        StatusCode::NOT_FOUND.into_response() // case when its not in the deployed folder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, extract::Path};

    #[tokio::test]
    async fn serves_application_assets_with_content_types() {
        let response = static_handler(Path("index.js".to_owned()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()[header::CONTENT_TYPE]
