use axum::{
    http::{header, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

pub fn static_routes() -> Router {
    Router::new().fallback(get(serve_static))
}

async fn serve_static(uri: Uri) -> Response {
    match crate::asset_for_path(uri.path()) {
        Some(data) => (
            [(header::CONTENT_TYPE, crate::mime_for_path(uri.path()))],
            data.into_owned(),
        )
            .into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}
