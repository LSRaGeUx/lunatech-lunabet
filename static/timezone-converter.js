/**
 * Converts UTC timestamps to the user's local timezone.
 * Uses the OS timezone (VPN-safe, no IP geolocation). Keeps the
 * server-side numeric format "<weekday> 11/06 - 19:00" (timed) or
 * "<weekday> 11/06" (date-only, finished matches); the weekday name
 * follows the selected app language (fr/en).
 * Re-runs on htmx swaps so injected match cards convert too.
 */
function convertKickoffs(root) {
  const scope = root || document;
  const kickoffElements = scope.querySelectorAll('[data-kickoff]');

  kickoffElements.forEach(el => {
    const date = new Date(el.dataset.kickoff);

    // Use the OS timezone; weekday name follows the app language.
    const userTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
    const locale = document.documentElement.lang || 'en-GB';

    // Date-only entries (finished matches) omit the time
    const dateOnly = el.dataset.dateOnly !== undefined;

    const opts = {
      weekday: 'short',
      month: '2-digit',
      day: '2-digit',
      timeZone: userTz,
      hour12: false
    };
    if (!dateOnly) {
      opts.hour = '2-digit';
      opts.minute = '2-digit';
    }

    const parts = new Intl.DateTimeFormat(locale, opts).formatToParts(date)
      .reduce((acc, p) => { acc[p.type] = p.value; return acc; }, {});

    let text = `${parts.weekday} ${parts.day}/${parts.month}`;
    if (!dateOnly) {
      text += ` - ${parts.hour}:${parts.minute}`;
    }
    el.textContent = text;
  });
}

document.addEventListener('DOMContentLoaded', () => convertKickoffs(document));
document.body.addEventListener('htmx:afterSwap', (e) => convertKickoffs(e.target));
