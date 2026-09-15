mod assets;
pub mod explorer;
pub mod home;
pub mod scan;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use log::error;

pub struct TemplateError(pub askama::Error);

impl IntoResponse for TemplateError {
    fn into_response(self) -> Response {
        error!("Template rendering error: {}", self.0);
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}

impl From<askama::Error> for TemplateError {
    fn from(err: askama::Error) -> Self {
        TemplateError(err)
    }
}
