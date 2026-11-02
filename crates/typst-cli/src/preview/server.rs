//! HTTP/WS host: serves the viewer and the `/ws` data plane.

use std::sync::Arc;

use axum::Router;
use axum::response::IntoResponse;
use axum::routing::get;

/// Shared host state: one compile pipeline, many viewers.
pub struct AppState;

impl AppState {
    fn router(self: Arc<Self>) -> Router {
        Router::new()
            .route("/", get(Self::viewer_handler))
            .route("/ws", get(Self::ws_handler))
            .fallback_service(get(Self::static_asset))
            .with_state(self)
    }

    async fn viewer_handler() -> impl IntoResponse {
        Self::static_asset(axum::http::Uri::from_static("/index.html")).await
    }

    async fn static_asset(uri: axum::http::Uri) -> impl IntoResponse {
        let path = uri.path().trim_start_matches('/');
        let Some(bytes) = holotyp_preview_viewer::get_file(path) else {
            return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
        };
        ([(axum::http::header::CONTENT_TYPE, mime_of(path))], bytes.to_vec())
            .into_response()
    }

    /// The data plane: handshake, resync, then live frames.
    async fn ws_handler() -> impl IntoResponse {
        axum::http::StatusCode::NOT_IMPLEMENTED
    }
}

/// Bind and start the accept loop.
pub async fn serve(host: &str, port: u16) -> std::io::Result<SocketAddr> {
    let state = Arc::new(AppState);

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let bound = listener.local_addr()?;
    eprintln!("serving at http://{host}:{}", bound.port());
    let app = state.router();
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("server error: {err}");
        }
    });
    Ok(bound)
}

/// The URL of a served preview, as a browser on this machine should open it.
pub fn browser_url(bound: SocketAddr) -> String {
    if bound.ip().is_unspecified() {
        return format!("http://127.0.0.1:{}", bound.port());
    }
    format!("http://{bound}")
}

/// The content type of one embedded viewer asset, by file extension.
fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("json") => "application/json",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
