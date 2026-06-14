// Register the service worker so LunaBet is installable and has an offline
// shell. Served from /sw.js (root scope) so it controls the whole app.
if ('serviceWorker' in navigator) {
    window.addEventListener('load', () => {
        navigator.serviceWorker.register('/sw.js').catch((err) => {
            console.warn('LunaBet: service worker registration failed', err);
        });
    });
}
