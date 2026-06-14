// Web Push opt-in UI (spec 08, part 2). Drives the notifications card on the
// profile page: request permission, subscribe via the service worker's
// PushManager, and keep the server in sync. Server-side delivery lives in
// src/webpush.rs; the service worker that renders the push lives in sw.js.
(() => {
    const card = document.getElementById('push-card');
    if (!card) return;

    const statusEl = document.getElementById('push-status');
    const enableBtn = document.getElementById('push-enable');
    const disableBtn = document.getElementById('push-disable');

    // iOS only exposes Web Push to an installed PWA (16.4+); plain Safari tabs
    // report no PushManager. Bail with a clear note rather than a dead button.
    const supported =
        'serviceWorker' in navigator &&
        'PushManager' in window &&
        'Notification' in window;

    const show = (el) => el && el.removeAttribute('hidden');
    const hide = (el) => el && el.setAttribute('hidden', '');

    // state: 'on' | 'off' | 'error'
    const setStatus = (msg, state) => {
        if (!statusEl) return;
        statusEl.textContent = msg;
        statusEl.classList.toggle('is-error', state === 'error');
        statusEl.classList.toggle('is-on', state === 'on');
        show(statusEl);
    };

    if (!supported) {
        setStatus(
            document.documentElement.lang === 'fr'
                ? "Ton navigateur ne supporte pas les notifications push. Sur iPhone, installe d'abord l'app sur l'écran d'accueil."
                : "Your browser doesn't support push notifications. On iPhone, install the app to your home screen first."
        );
        return;
    }

    const fr = document.documentElement.lang === 'fr';
    const t = (frTxt, enTxt) => (fr ? frTxt : enTxt);

    // base64url string -> Uint8Array, for applicationServerKey.
    const urlBase64ToUint8Array = (base64String) => {
        const padding = '='.repeat((4 - (base64String.length % 4)) % 4);
        const base64 = (base64String + padding).replace(/-/g, '+').replace(/_/g, '/');
        const raw = atob(base64);
        const out = new Uint8Array(raw.length);
        for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
        return out;
    };

    const getRegistration = async () =>
        (await navigator.serviceWorker.getRegistration()) ||
        (await navigator.serviceWorker.ready);

    // Reflect the current subscription state in the buttons.
    const refresh = async () => {
        if (Notification.permission === 'denied') {
            setStatus(
                t(
                    'Bloquées dans les réglages du navigateur.',
                    'Blocked in your browser settings.'
                ),
                'error'
            );
            hide(enableBtn);
            hide(disableBtn);
            return;
        }
        const reg = await getRegistration();
        const sub = reg ? await reg.pushManager.getSubscription() : null;
        if (sub) {
            setStatus(t('Activées sur cet appareil', 'On for this device'), 'on');
            hide(enableBtn);
            show(disableBtn);
        } else {
            setStatus(t('Désactivées sur cet appareil', 'Off for this device'), 'off');
            show(enableBtn);
            hide(disableBtn);
        }
    };

    const subscribe = async () => {
        enableBtn.disabled = true;
        try {
            const permission = await Notification.requestPermission();
            if (permission !== 'granted') {
                setStatus(t('Permission refusée.', 'Permission denied.'), 'error');
                return;
            }
            const keyResp = await fetch('/push/public-key');
            if (!keyResp.ok) throw new Error('public key unavailable');
            const key = (await keyResp.text()).trim();

            const reg = await getRegistration();
            const sub = await reg.pushManager.subscribe({
                userVisibleOnly: true,
                applicationServerKey: urlBase64ToUint8Array(key),
            });

            const resp = await fetch('/push/subscribe', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(sub),
            });
            if (!resp.ok) throw new Error('subscribe failed');
            setStatus(t('Activées 🎉', 'Enabled 🎉'), 'on');
        } catch (err) {
            console.warn('LunaBet push subscribe failed', err);
            setStatus(t("Impossible d'activer.", 'Could not enable.'), 'error');
        } finally {
            enableBtn.disabled = false;
            await refresh();
        }
    };

    const unsubscribe = async () => {
        disableBtn.disabled = true;
        try {
            const reg = await getRegistration();
            const sub = reg ? await reg.pushManager.getSubscription() : null;
            if (sub) {
                await fetch('/push/unsubscribe', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ endpoint: sub.endpoint }),
                });
                await sub.unsubscribe();
            }
        } catch (err) {
            console.warn('LunaBet push unsubscribe failed', err);
        } finally {
            disableBtn.disabled = false;
            await refresh();
        }
    };

    if (enableBtn) enableBtn.addEventListener('click', subscribe);
    if (disableBtn) disableBtn.addEventListener('click', unsubscribe);

    refresh();
})();
