// Kickoff countdown for LunaBet.
//
// Adds a live "time left to bet" indicator to every open match card and turns
// urgent under an hour, to nudge users to place their bet before kickoff. When
// the clock hits zero the card locks client-side (inputs disabled) without a
// reload; the server stays the authority on whether a bet is actually accepted
// (see Match::is_open_for_bets).
//
// Pure vanilla, no deps. Drives off the existing data-kickoff (RFC3339) already
// rendered on each card, and the app language on <html lang>. Re-runs on htmx
// swaps so freshly injected cards get a countdown too.

(function () {
    'use strict';

    var URGENT_MS = 60 * 60 * 1000; // under one hour: switch to urgent styling
    var timer = null;

    function ready(fn) {
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', fn, { once: true });
        } else {
            fn();
        }
    }

    function isFr() {
        return (document.documentElement.lang || 'en').toLowerCase().indexOf('fr') === 0;
    }

    // Compact duration: "3d 4h" / "3j 4h", "2h 05m", "23 min", or the
    // "less than a minute" wording. Returns null once the deadline has passed.
    function duration(ms, fr) {
        var total = Math.floor(ms / 1000);
        if (total <= 0) return null;
        var mins = Math.floor(total / 60);
        var days = Math.floor(mins / 1440);
        var hours = Math.floor((mins % 1440) / 60);
        var rem = mins % 60;
        if (days >= 1) return fr ? (days + 'j ' + hours + 'h') : (days + 'd ' + hours + 'h');
        if (mins >= 60) return hours + 'h ' + String(rem).padStart(2, '0') + 'm';
        if (mins >= 1) return mins + ' min';
        return fr ? "moins d'1 min" : 'less than 1 min';
    }

    function label(ms, fr) {
        var d = duration(ms, fr);
        if (d === null) return null;
        if (ms < URGENT_MS) {
            return fr ? ('Plus que ' + d + ' pour parier !') : ('Only ' + d + ' left to bet!');
        }
        return fr ? ('Coup d’envoi dans ' + d) : ('Kickoff in ' + d);
    }

    // The editable bet form of an open match, or null when the card is a
    // finished/closed match (no form, or disabled inputs).
    function openForm(card) {
        var input = card.querySelector('form.bet-form input[name="home_score"]');
        if (!input || input.disabled) return null;
        return input.form || card.querySelector('form.bet-form');
    }

    function ensureBadge(card) {
        var anchor = card.querySelector('[data-kickoff]');
        if (!anchor) return null;
        var badge = card.querySelector('.countdown');
        if (!badge) {
            badge = document.createElement('span');
            badge.className = 'countdown';
            anchor.insertAdjacentElement('afterend', badge);
        }
        return { badge: badge, kickoff: new Date(anchor.dataset.kickoff).getTime() };
    }

    function lock(card, form, fr) {
        card.classList.add('locked');
        form.querySelectorAll('input, button').forEach(function (el) { el.disabled = true; });
        var badge = card.querySelector('.countdown');
        if (badge) {
            badge.textContent = fr ? 'Paris clos' : 'Bets closed';
            badge.classList.remove('countdown-urgent');
            badge.classList.add('countdown-closed');
        }
    }

    // Update one card. Returns true while it still has a running countdown.
    function tickCard(card, fr) {
        var form = openForm(card);
        if (!form) return false;
        var info = ensureBadge(card);
        if (!info) return false;
        var remaining = info.kickoff - Date.now();
        if (remaining <= 0) {
            lock(card, form, fr);
            return false;
        }
        info.badge.textContent = label(remaining, fr);
        info.badge.classList.toggle('countdown-urgent', remaining < URGENT_MS);
        return true;
    }

    function tickAll() {
        var fr = isFr();
        document.querySelectorAll('article.match').forEach(function (card) { tickCard(card, fr); });
    }

    function hasOpen() {
        return !!document.querySelector('form.bet-form input[name="home_score"]:not([disabled])');
    }

    function start() {
        tickAll();
        if (!timer && hasOpen()) timer = setInterval(tickAll, 1000);
    }

    ready(start);
    document.body.addEventListener('htmx:afterSwap', start);
})();
