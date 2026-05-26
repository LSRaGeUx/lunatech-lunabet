use std::hash::{DefaultHasher, Hash, Hasher};

use uuid::Uuid;

/// The pool of Captain Tsubasa inspired SVG avatars shipped in
/// `static/characters/`. Each registered user is deterministically assigned
/// one of these; multiple users will share a character once we have more
/// than `CHARACTERS.len()` registrations.
pub const CHARACTERS: &[&str] = &[
    "olivier",
    "misaki",
    "genzo",
    "hyuga",
    "schneider",
    "pierre",
    "roberto",
    "ishizaki",
];

/// Pick the character slug for a given user. Stable across boots because
/// `DefaultHasher` seeds from a fixed value for the same input — a user
/// keeps their avatar between deploys.
pub fn slug_for(user_id: Uuid) -> &'static str {
    let mut h = DefaultHasher::new();
    user_id.hash(&mut h);
    let idx = (h.finish() as usize) % CHARACTERS.len();
    CHARACTERS[idx]
}

/// Resolved public path the browser fetches.
pub fn path_for(user_id: Uuid) -> String {
    format!("/static/characters/ch-{}.svg", slug_for(user_id))
}
