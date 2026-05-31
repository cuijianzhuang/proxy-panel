//! Serve the built React SPA from inside the binary.
//!
//! `web/dist/` is embedded at compile time via `rust-embed`. Requests for
//! actual files (`/assets/index-*.js`, `/favicon.svg`, …) get served with
//! the right Content-Type. Anything else falls back to `index.html` so
//! React Router's client-side routes work on hard refresh.
//!
//! The API routes are mounted before this fallback, so `/api/*` and `/sub/*`
//! never enter this handler.

use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct WebAssets;

/// Axum fallback handler.
pub async fn handler(uri: Uri) -> Response {
    let path = uri.path();

    // Reserved prefixes: a typo'd API path should yield a clean JSON-ish 404
    // rather than a misleading 200 + index.html.
    if path.starts_with("/api/") || path.starts_with("/sub/") || path.starts_with("/s/") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    let stripped = path.trim_start_matches('/');
    let candidate = if stripped.is_empty() { "index.html" } else { stripped };

    if let Some(file) = WebAssets::get(candidate) {
        return file_response(candidate, file.data.into_owned());
    }

    // SPA fallback: anything not matched is presumed to be a client-side route.
    if let Some(idx) = WebAssets::get("index.html") {
        return file_response("index.html", idx.data.into_owned());
    }

    // Missing entirely — happens before the first `npm run build`.
    (
        StatusCode::NOT_FOUND,
        "frontend assets not built; run `npm run build` in web/",
    )
        .into_response()
}

fn file_response(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path.starts_with("assets/") {
        // Vite's hashed asset filenames let us cache aggressively.
        "public, max-age=31536000, immutable"
    } else {
        // index.html + everything else should always revalidate.
        "no-cache, no-store, must-revalidate"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .header(header::CACHE_CONTROL, HeaderValue::from_static(cache))
        .body(Body::from(bytes))
        .unwrap()
}
