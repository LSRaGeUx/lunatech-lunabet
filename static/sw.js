// LunaBet service worker. Two jobs:
//  1. Make the app installable + give it a small offline shell by caching the
//     static assets (cache-first). Authenticated pages are NOT cached, so we
//     never serve a stale logged-in view.
//  2. Be ready to receive Web Push (the backend push sender is a follow-up;
//     the handlers below already render a notification from the payload).
const CACHE = 'lunabet-static-v1';
const SHELL = ['/static/style.css', '/static/favicon.svg'];

self.addEventListener('install', (event) => {
    event.waitUntil(
        caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting())
    );
});

self.addEventListener('activate', (event) => {
    event.waitUntil(
        caches.keys()
            .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
            .then(() => self.clients.claim())
    );
});

self.addEventListener('fetch', (event) => {
    const req = event.request;
    if (req.method !== 'GET') return;
    const url = new URL(req.url);
    // Cache-first for our own static assets only; let everything else hit the
    // network untouched so logged-in pages are always fresh.
    if (url.origin === self.location.origin && url.pathname.startsWith('/static/')) {
        event.respondWith(
            caches.match(req).then((hit) => hit || fetch(req).then((res) => {
                const copy = res.clone();
                caches.open(CACHE).then((c) => c.put(req, copy));
                return res;
            }))
        );
    }
});

// Web Push (payload: { title, body, url }). Wired now so enabling the backend
// sender later needs no service-worker change.
self.addEventListener('push', (event) => {
    let data = {};
    try { data = event.data ? event.data.json() : {}; } catch (_) { /* non-JSON payload */ }
    const title = data.title || 'LunaBet';
    event.waitUntil(self.registration.showNotification(title, {
        body: data.body || '',
        icon: '/static/favicon.svg',
        badge: '/static/favicon.svg',
        data: { url: data.url || '/today' },
    }));
});

self.addEventListener('notificationclick', (event) => {
    event.notification.close();
    const target = (event.notification.data && event.notification.data.url) || '/today';
    event.waitUntil(self.clients.openWindow(target));
});
