// Score celebration for LunaBet.
//
// When the user comes back and the scoring job has settled one of their
// predictions as a win, the Today page injects a small JSON payload in
// #celebrate-data (built server-side in routes/today.rs). This fires a single
// burst-and-message overlay so the win lands with a bit of dopamine. Each win
// is celebrated once: the server marks results as seen when it renders.
//
// Pure vanilla, no deps. Display-ready messages come from the server so all
// i18n stays in Rust.

(function () {
    'use strict';

    function ready(fn) {
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', fn, { once: true });
        } else {
            fn();
        }
    }

    function parseData() {
        var el = document.getElementById('celebrate-data');
        if (!el) return null;
        try {
            return JSON.parse(el.textContent);
        } catch (e) {
            return null;
        }
    }

    // Normalise both payload shapes into a flat list of { message, level }.
    function toItems(data) {
        if (!data) return [];
        if (data.mode === 'aggregate') {
            return [{ message: data.message, level: 'exact' }];
        }
        return (data.wins || []).map(function (w) {
            return { message: w.message, level: w.level === 'exact' ? 'exact' : 'outcome' };
        });
    }

    function hasExact(items) {
        return items.some(function (i) { return i.level === 'exact'; });
    }

    function celebrate(items) {
        if (!items.length) return;

        var overlay = document.createElement('div');
        overlay.className = 'celebrate-overlay' + (hasExact(items) ? ' celebrate-exact' : '');
        overlay.setAttribute('role', 'status');

        var burst = document.createElement('img');
        burst.src = '/static/manga-burst.svg';
        burst.alt = '';
        burst.className = 'celebrate-burst';
        overlay.appendChild(burst);

        var list = document.createElement('div');
        list.className = 'celebrate-messages';
        items.forEach(function (it, idx) {
            var line = document.createElement('div');
            line.className = 'celebrate-line celebrate-' + (it.level === 'exact' ? 'exact' : 'outcome');
            line.style.animationDelay = (idx * 0.18) + 's';
            line.textContent = it.message;
            list.appendChild(line);
        });
        overlay.appendChild(list);

        document.body.appendChild(overlay);

        function dismiss() {
            overlay.classList.add('celebrate-out');
            setTimeout(function () { overlay.remove(); }, 500);
        }
        overlay.addEventListener('click', dismiss);
        setTimeout(dismiss, 3600 + items.length * 250);
    }

    ready(function () {
        celebrate(toItems(parseData()));
    });
})();
