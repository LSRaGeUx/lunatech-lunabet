#!/usr/bin/env bash
# Capture screenshots of every key page into docs/screenshots/.
#
# Requirements:
#  - Google Chrome installed (macOS path hard-coded; override with $CHROME env var)
#  - LunaBet server running locally on http://127.0.0.1:3000 with seeded data
#    (run `cargo run -- seed` then `cargo run`)
#
# Usage:  ./scripts/screenshots.sh
set -euo pipefail

CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/docs/screenshots"
URL_BASE="http://127.0.0.1:3000"
LOGIN_EMAIL="${LOGIN_EMAIL:-nicolas.leroux@lunatech.com}"
WIDTH="${WIDTH:-1280}"
HEIGHT="${HEIGHT:-900}"

if [ ! -x "$CHROME" ] && ! command -v "$CHROME" >/dev/null 2>&1; then
    echo "Chrome not found at $CHROME (override via CHROME env var)" >&2
    exit 1
fi

if ! curl -sf "$URL_BASE/" >/dev/null 2>&1; then
    echo "Server not reachable at $URL_BASE — start it first (cargo run)" >&2
    exit 1
fi

mkdir -p "$OUT"
PROFILE="$(mktemp -d -t lunabet-shots-XXXXXX)"
trap 'rm -rf "$PROFILE"' EXIT

shot() {
    local file="$1"; shift
    local path="$1"; shift
    local wait="${1:-2}"
    local hard_kill=$((wait + 8))
    echo "[shot] $path -> $file (wait ${wait}s)"
    # Use legacy --headless (not --headless=new) — it exits more reliably.
    # Run in background and kill after a hard deadline if --virtual-time-budget
    # doesn't terminate the process (e.g. Three.js raf loops).
    "$CHROME" \
        --headless \
        --enable-webgl \
        --use-gl=angle \
        --user-data-dir="$PROFILE" \
        --no-first-run --no-default-browser-check \
        --hide-scrollbars \
        --force-device-scale-factor=2 \
        --window-size="$WIDTH,$HEIGHT" \
        --virtual-time-budget=$((wait * 1000)) \
        --screenshot="$OUT/$file" \
        "$URL_BASE$path" >/dev/null 2>&1 &
    local pid=$!
    local elapsed=0
    while kill -0 "$pid" 2>/dev/null; do
        sleep 1
        elapsed=$((elapsed + 1))
        if [ "$elapsed" -ge "$hard_kill" ]; then
            echo "  (hard-killing after ${elapsed}s)"
            kill -9 "$pid" 2>/dev/null || true
            # also clean up any chrome helpers tied to our profile
            pkill -9 -f "$(basename "$PROFILE")" 2>/dev/null || true
            break
        fi
    done
    wait "$pid" 2>/dev/null || true
}

# 0) Set FR language cookie first (so the rest is in French)
shot "_lang.png" "/lang/fr" 1
rm -f "$OUT/_lang.png"

# 1) Home (not logged in)
shot "01-home.png" "/" 2

# 2) Login form
shot "02-login.png" "/login" 1

# 3) Dev page (test users)
shot "03-dev.png" "/dev" 1

# 4) Log in as Nicolas — sets session cookie in the temp profile
shot "_login.png" "/dev/login?email=$LOGIN_EMAIL" 1
rm -f "$OUT/_login.png"

# 5) Matches (vignettes, mini ball spinning in topbar)
shot "04-matches.png" "/matches" 3

# 6) Leaderboard — pass ?shot=1 so the 3D goal scene freezes mid-flight
shot "05-leaderboard.png" "/leaderboard?shot=1" 4

# 7) Stake page
shot "06-stake.png" "/stake" 2

# 8) Admin panel
shot "07-admin-stakes.png" "/admin/stakes" 2

# 9) Switch language to EN and re-shoot the leaderboard
shot "_lang.png" "/lang/en" 1
rm -f "$OUT/_lang.png"
shot "08-leaderboard-en.png" "/leaderboard?shot=1" 4

echo ""
echo "Screenshots written to $OUT"
ls -lh "$OUT"
