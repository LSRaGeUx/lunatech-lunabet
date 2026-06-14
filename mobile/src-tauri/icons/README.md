# App icons

These are generated, not committed (see `mobile/.gitignore`). Produce them from
the LunaBet mark before the first build:

```sh
cd mobile/src-tauri
# Start from a square PNG (>= 1024x1024). The brand asset is static/favicon.svg
# at the repo root; export it to PNG first (e.g. with rsvg-convert or any editor).
cargo tauri icon path/to/lunabet-1024.png
```

This writes `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`,
`icon.ico` and the platform icon sets referenced by `tauri.conf.json`.
