use axum::extract::FromRequestParts;
use axum::http::header::ACCEPT_LANGUAGE;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::Response;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Locale {
    Fr,
    #[default]
    En,
}

impl Locale {
    pub fn code(self) -> &'static str {
        match self {
            Locale::Fr => "fr",
            Locale::En => "en",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        match s {
            "fr" => Some(Locale::Fr),
            "en" => Some(Locale::En),
            _ => None,
        }
    }

    /// Returns the French string when the active locale is FR,
    /// otherwise the English one. Used inline in templates:
    /// `{{ loc.f("Bonjour", "Hello") }}`
    pub fn f(self, fr: &'static str, en: &'static str) -> &'static str {
        match self {
            Locale::Fr => fr,
            Locale::En => en,
        }
    }

    pub fn from_accept_language(header_value: &str) -> Self {
        // Very lightweight Accept-Language parser: pick the first tag that
        // starts with "fr" or "en"; default to English.
        for tag in header_value.split(',') {
            let tag = tag.trim().split([';', '-']).next().unwrap_or("").to_ascii_lowercase();
            if tag.starts_with("fr") {
                return Locale::Fr;
            }
            if tag.starts_with("en") {
                return Locale::En;
            }
        }
        Locale::En
    }
}

pub const LANG_COOKIE: &str = "lb_lang";

impl<S> FromRequestParts<S> for Locale
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // 1) explicit cookie wins
        if let Some(cookie_header) = parts.headers.get(axum::http::header::COOKIE) {
            if let Ok(s) = cookie_header.to_str() {
                for kv in s.split(';') {
                    let mut split = kv.trim().splitn(2, '=');
                    let k = split.next().unwrap_or("");
                    let v = split.next().unwrap_or("");
                    if k == LANG_COOKIE {
                        if let Some(l) = Locale::from_code(v) {
                            return Ok(l);
                        }
                    }
                }
            }
        }
        // 2) Accept-Language fallback
        if let Some(al) = parts.headers.get(ACCEPT_LANGUAGE) {
            if let Ok(s) = al.to_str() {
                return Ok(Locale::from_accept_language(s));
            }
        }
        Ok(Locale::default())
    }
}


#[allow(dead_code)]
fn _ensure_response_compiles(_r: Response) {}
