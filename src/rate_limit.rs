use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// In-process sliding-window rate limiter for the public signup endpoint.
/// Survives only for the lifetime of the process: good enough for a single
/// Clever Cloud instance, replace with a Redis-backed counter when we scale
/// horizontally.
#[derive(Clone)]
pub struct SignupRateLimiter {
    inner: Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>,
    window: Duration,
    max_hits: usize,
}

impl SignupRateLimiter {
    pub fn new(window: Duration, max_hits: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            window,
            max_hits,
        }
    }

    /// Records a hit and reports whether the caller is still within the
    /// allowed quota. Returns `true` if the request can proceed.
    pub fn check_and_record(&self, ip: IpAddr) -> bool {
        let mut map = self.inner.lock().expect("signup limiter mutex poisoned");
        let cutoff = Instant::now() - self.window;
        let entry = map.entry(ip).or_default();
        entry.retain(|t| *t > cutoff);
        if entry.len() >= self.max_hits {
            return false;
        }
        entry.push(Instant::now());
        true
    }

    /// Drop empty buckets to keep the map from growing forever. Called from
    /// the periodic cleanup task.
    pub fn purge_empty(&self) {
        let mut map = self.inner.lock().expect("signup limiter mutex poisoned");
        let cutoff = Instant::now() - self.window;
        map.retain(|_, entry| {
            entry.retain(|t| *t > cutoff);
            !entry.is_empty()
        });
    }
}

/// Generic rate limiter for any endpoint. Uses a per-endpoint map of IP -> timestamps.
#[derive(Clone)]
pub struct EndpointRateLimiter {
    inner: Arc<Mutex<HashMap<String, HashMap<IpAddr, Vec<Instant>>>>>,
    window: Duration,
    max_hits: usize,
}

impl EndpointRateLimiter {
    pub fn new(window: Duration, max_hits: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            window,
            max_hits,
        }
    }

    /// Records a hit for the given endpoint and IP, returns true if under limit.
    pub fn check_and_record(&self, endpoint: &str, ip: IpAddr) -> bool {
        let mut map = self.inner.lock().expect("endpoint limiter mutex poisoned");
        let cutoff = Instant::now() - self.window;
        let endpoint_map = map.entry(endpoint.to_string()).or_default();
        let entry = endpoint_map.entry(ip).or_default();
        entry.retain(|t| *t > cutoff);
        if entry.len() >= self.max_hits {
            return false;
        }
        entry.push(Instant::now());
        true
    }

    /// Drop empty buckets to keep memory usage in check.
    pub fn purge_empty(&self) {
        let mut map = self.inner.lock().expect("endpoint limiter mutex poisoned");
        let cutoff = Instant::now() - self.window;
        map.retain(|_, endpoint_map| {
            endpoint_map.retain(|_, entry| {
                entry.retain(|t| *t > cutoff);
                !entry.is_empty()
            });
            !endpoint_map.is_empty()
        });
    }
}

/// Best-effort client IP extraction. Behind a reverse proxy (Clever Cloud,
/// Cloudflare) the real IP is in `X-Forwarded-For`; we trust the first hop
/// because the proxy chain is ours. Returns `None` in local dev when no
/// proxy header is set; callers should fail open in that case.
pub fn client_ip(headers: &axum::http::HeaderMap) -> Option<IpAddr> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse().ok())
        })
}


