//! HTTP and WebSocket handlers for the sshxx web interface.

use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{any, get_service};
use axum::Router;
use http::header::{HeaderValue, CACHE_CONTROL, CONTENT_SECURITY_POLICY, X_FRAME_OPTIONS};
use tower_http::services::{ServeDir, ServeFile};

use crate::ServerState;

pub mod protocol;
mod socket;

/// Returns the web application server, routed with Axum.
pub fn app() -> Router<Arc<ServerState>> {
    let root_spa = ServeFile::new("build/spa.html")
        .precompressed_gzip()
        .precompressed_br();

    // Serves static SvelteKit build files.
    let static_files = ServeDir::new("build")
        .precompressed_gzip()
        .precompressed_br()
        .fallback(root_spa);
    let immutable_files = ServeDir::new("build/_app/immutable")
        .precompressed_gzip()
        .precompressed_br();

    Router::new()
        .nest("/api", backend())
        // Missing hashed assets must be a 404 instead of the SPA HTML fallback;
        // Vite can then recover a stale page through `vite:preloadError`.
        .nest_service("/_app/immutable", get_service(immutable_files))
        .fallback_service(get_service(static_files))
        .layer(middleware::from_fn(static_cache_headers))
}

async fn static_cache_headers(request: Request, next: Next) -> Response {
    let policy = cache_policy(request.uri().path());
    let mut response = next.run(request).await;
    if policy.is_some() {
        response.headers_mut().insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none'"),
        );
        response
            .headers_mut()
            .insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    }
    if policy.is_some() && !response.status().is_success() {
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    } else if let Some(policy) = policy {
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static(policy));
    }
    response
}

fn cache_policy(path: &str) -> Option<&'static str> {
    if path.starts_with("/api/") {
        None
    } else if path.starts_with("/_app/immutable/") {
        Some("public, max-age=31536000, immutable")
    } else {
        Some("no-cache")
    }
}

/// Routes for the backend web API server.
fn backend() -> Router<Arc<ServerState>> {
    Router::new().route("/s/{name}", any(socket::get_session_ws))
}

#[cfg(test)]
mod tests {
    use super::cache_policy;

    #[test]
    fn assigns_safe_static_asset_cache_policies() {
        assert_eq!(cache_policy("/s/dev"), Some("no-cache"));
        assert_eq!(
            cache_policy("/_app/immutable/chunks/editor.js"),
            Some("public, max-age=31536000, immutable")
        );
        assert_eq!(cache_policy("/api/s/dev"), None);
    }
}
