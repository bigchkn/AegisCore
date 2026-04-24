use axum::{
    http::{header, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

pub fn static_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().fallback(get(serve_static))
}

async fn serve_static(uri: Uri) -> Response {
    match crate::asset_for_path_with_mime(uri.path()) {
        Some((data, mime)) => ([(header::CONTENT_TYPE, mime)], data.into_owned()).into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn get_root_returns_embedded_index() {
        let response = super::static_routes::<()>()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn client_route_falls_back_to_index() {
        let response = super::static_routes::<()>()
            .oneshot(
                Request::builder()
                    .uri("/agents/some-agent-id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
    }
}
