
#[cfg(feature = "ssr")]
pub async fn file_handler(
    uri: axum::http::Uri,
    axum::extract::State(options): axum::extract::State<leptos::LeptosOptions>,
    _req: axum::http::Request<axum::body::Body>,
) -> axum::response::Response {
    let root = options.site_root.clone();
    let res = get_static_file(uri.clone(), &root).await;

    use axum::response::IntoResponse;
    match res {
        Ok(response) => response.into_response(),
        Err((status, error_msg)) => {
            leptos::logging::log!("file_handler error: {}", error_msg);
            status.into_response()
        },
    }
}

#[cfg(feature = "ssr")]
async fn get_static_file(
    uri: axum::http::Uri,
    root: &str,
) -> Result<axum::http::Response<axum::body::Body>, (axum::http::StatusCode, String)> {
    let req = axum::http::Request::builder()
        .uri(uri.clone())
        .body(axum::body::Body::empty())
        .unwrap();
    // `ServeDir` implements `tower::Service` so we can call it with `tower::ServiceExt::oneshot`
    // This path is relative to the cargo root
    use axum::response::IntoResponse;
    use tower::ServiceExt;
    match tower_http::services::ServeDir::new(root).oneshot(req).await {
        Ok(res) => Ok(res.into_response()),
        Err(err) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {err}"),
        )),
    }
}