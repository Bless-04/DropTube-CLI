use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "public/"]
struct Assets;

pub async fn static_handler(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    if let Some(content) = Assets::get(&path) {
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
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
                .to_str()
                .expect("content type")
                .contains("javascript")
        );
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("asset body");
        assert!(String::from_utf8_lossy(&body).contains("IntersectionObserver"));
    }

    #[tokio::test]
    async fn missing_assets_return_not_found() {
        let response = static_handler(Path("missing.js".to_owned()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
