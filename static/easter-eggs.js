// Captain Tsubasa easter eggs for LunaBet.
//
//  - Click the hero 3D ball : Olivier Atton pops up with a speech bubble.
//  - Click a +3 pts badge   : a tiger streaks across the screen.
//  - Konami code            : full-screen Catapulte Infernale.
//
// Pure vanilla, no deps, all artwork inline so this stays a single file load.

(function () {
    'use strict';

    function ready(fn) {
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', fn, { once: true });
        } else {
            fn();
        }
    }

    // ------------------------------------------------------------------
    // 1. Olivier Atton on hero-ball click
    // ------------------------------------------------------------------

    function spawnOlivier(originRect) {
        if (document.querySelector('.ee-olivier-active')) return;
        const wrap = document.createElement('div');
        wrap.className = 'ee-olivier-active';
        wrap.innerHTML =
            '<img src="/static/ee-tsubasa.svg" alt="" class="ee-olivier-char">' +
            '<div class="ee-bubble">« Le ballon est ton ami ! »</div>';

        // Anchor near the clicked ball.
        const top = window.scrollY + originRect.bottom + 12;
        const left = window.scrollX + originRect.left;
        wrap.style.top = top + 'px';
        wrap.style.left = left + 'px';

        document.body.appendChild(wrap);
        setTimeout(function () { wrap.classList.add('ee-fade-out'); }, 2800);
        setTimeout(function () { wrap.remove(); }, 3400);
    }

    function bindHeroBall() {
        document.querySelectorAll('.ball-hero').forEach(function (canvas) {
            canvas.style.cursor = 'pointer';
            canvas.addEventListener('click', function (ev) {
                spawnOlivier(canvas.getBoundingClientRect());
            });
        });
    }

    // ------------------------------------------------------------------
    // 2. Tiger streak on pts-3 click
    // ------------------------------------------------------------------

    function spawnTigerStreak() {
        if (document.querySelector('.ee-tiger-streak')) return;
        const streak = document.createElement('div');
        streak.className = 'ee-tiger-streak';
        streak.innerHTML =
            '<span class="ee-tiger-emoji">🐯</span>' +
            '<span class="ee-tiger-text">TIR DU TIGRE !</span>';
        document.body.appendChild(streak);
        setTimeout(function () { streak.remove(); }, 1400);
    }

    function bindPts3() {
        document.addEventListener('click', function (ev) {
            const badge = ev.target.closest && ev.target.closest('.pts-3');
            if (!badge) return;
            spawnTigerStreak();
        });
    }

    // ------------------------------------------------------------------
    // 3. Konami code → Catapulte Infernale
    // ------------------------------------------------------------------

    const KONAMI = [
        'ArrowUp', 'ArrowUp', 'ArrowDown', 'ArrowDown',
        'ArrowLeft', 'ArrowRight', 'ArrowLeft', 'ArrowRight',
        'b', 'a',
    ];

    function spawnCatapulte() {
        if (document.querySelector('.ee-catapulte')) return;
        const overlay = document.createElement('div');
        overlay.className = 'ee-catapulte';
        overlay.innerHTML =
            '<img src="/static/manga-burst.svg" class="ee-catapulte-burst" alt="">' +
            '<div class="ee-catapulte-ball" aria-hidden="true">⚽</div>' +
            '<div class="ee-catapulte-text">CATAPULTE INFERNALE&nbsp;!</div>';
        document.body.appendChild(overlay);
        setTimeout(function () { overlay.remove(); }, 2400);
    }

    function bindKonami() {
        let buf = [];
        document.addEventListener('keydown', function (ev) {
            // Ignore when the user is typing in an input/textarea.
            const tag = (ev.target && ev.target.tagName) || '';
            if (tag === 'INPUT' || tag === 'TEXTAREA' || ev.target.isContentEditable) {
                return;
            }
            const k = ev.key.length === 1 ? ev.key.toLowerCase() : ev.key;
            buf.push(k);
            if (buf.length > KONAMI.length) buf.shift();
            if (buf.length === KONAMI.length && buf.every(function (v, i) { return v === KONAMI[i]; })) {
                buf = [];
                spawnCatapulte();
            }
        });
    }

    ready(function () {
        bindHeroBall();
        bindPts3();
        bindKonami();
    });
})();
