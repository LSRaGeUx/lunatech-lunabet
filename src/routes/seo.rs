//! Search-engine plumbing: `/robots.txt` and `/sitemap.xml`. Both are served
//! at the site root (not under `/static`) and are host-aware so each tenant
//! subdomain and the marketing apex advertise their own URLs.

use axum::extract::State;
use axum::http::header::{HOST, CONTENT_TYPE};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

/// Best-effort absolute base URL for the current request: scheme from the
/// configured base URL, host from the request. Falls back to the configured
/// base URL when there's no usable Host header.
fn request_base(headers: &HeaderMap, state: &AppState) -> String {
    let host = headers.get(HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    if host.is_empty() {
        return state.cfg.base_url.trim_end_matches('/').to_string();
    }
    let local = host.starts_with("localhost") || host.starts_with("127.0.0.1");
    let scheme = if local { "http" } else { "https" };
    format!("{scheme}://{host}")
}

pub async fn robots(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let base = request_base(&headers, &state);
    // The whole app sits behind login; only the marketing/landing and signup
    // pages should be crawled. Everything user-specific is disallowed.
    let body = format!(
        "User-agent: *\n\
         Disallow: /today\n\
         Disallow: /matches\n\
         Disallow: /leaderboard\n\
         Disallow: /stake\n\
         Disallow: /admin\n\
         Disallow: /super-admin\n\
         Disallow: /dev\n\
         Disallow: /auth\n\
         Disallow: /login\n\
         Disallow: /lang/\n\
         Allow: /$\n\
         Allow: /signup\n\
         \n\
         Sitemap: {base}/sitemap.xml\n"
    );
    ([(CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

pub async fn sitemap(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let base = request_base(&headers, &state);
    let paths = ["/", "/signup"];
    let mut body = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for p in paths {
        body.push_str(&format!("  <url><loc>{base}{p}</loc></url>\n"));
    }
    body.push_str("</urlset>\n");
    ([(CONTENT_TYPE, "application/xml; charset=utf-8")], body).into_response()
}
