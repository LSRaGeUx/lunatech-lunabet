#!/usr/bin/env bash
# Capture the latest magic-link and match-reminder emails as they appear in Mailpit.
# Usage: ./scripts/screenshot-emails.sh
#
# Mailpit must be running on http://localhost:8025 (started via docker compose up -d).

set -euo pipefail

CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/docs/screenshots"
MAILPIT="${MAILPIT:-http://127.0.0.1:8025}"

mkdir -p "$OUT"
PROFILE="$(mktemp -d -t lunabet-mail-XXXXXX)"
trap 'rm -rf "$PROFILE"' EXIT

shot() {
    local file="$1"; shift
    local url="$1"; shift
    local wait="${1:-2}"
    local hard_kill=$((wait + 8))
    echo "[shot] $url -> $file"
    "$CHROME" --headless --enable-webgl --use-gl=angle \
        --user-data-dir="$PROFILE" \
        --no-first-run --no-default-browser-check --hide-scrollbars \
        --force-device-scale-factor=2 \
        --window-size=900,1000 \
        --virtual-time-budget=$((wait * 1000)) \
        --screenshot="$OUT/$file" "$url" >/dev/null 2>&1 &
    local pid=$!
    local elapsed=0
    while kill -0 "$pid" 2>/dev/null; do
        sleep 1; elapsed=$((elapsed + 1))
        if [ "$elapsed" -ge "$hard_kill" ]; then
            kill -9 "$pid" 2>/dev/null || true
            pkill -9 -f "$(basename "$PROFILE")" 2>/dev/null || true
            break
        fi
    done
    wait "$pid" 2>/dev/null || true
}

# Fetch latest message IDs by subject
ML_ID=$(curl -s "$MAILPIT/api/v1/messages?query=subject:\"connexion%20LunaBet\"&limit=1" \
    | python3 -c 'import sys,json; m=json.load(sys.stdin)["messages"]; print(m[0]["ID"] if m else "")')
REM_ID=$(curl -s "$MAILPIT/api/v1/messages?query=subject:%22Rappel%22&limit=1" \
    | python3 -c 'import sys,json; m=json.load(sys.stdin)["messages"]; print(m[0]["ID"] if m else "")')

if [ -z "$ML_ID" ]; then
    echo "No magic-link email found in Mailpit. POST something to /login first." >&2
fi
if [ -z "$REM_ID" ]; then
    echo "No reminder email found in Mailpit." >&2
fi

# Mailpit's preview iframe URL: /view/<id>.html (renders the HTML body, or text in pre)
[ -n "$ML_ID" ] && shot "09-email-magic-link.png" "$MAILPIT/view/$ML_ID.html" 2
[ -n "$REM_ID" ] && shot "10-email-reminder.png" "$MAILPIT/view/$REM_ID.html" 2

echo "Done."
ls -lh "$OUT" | grep -E "09-|10-"
