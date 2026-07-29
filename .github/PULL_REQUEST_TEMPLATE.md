## What changed

-

## Why

-

## Validation

- [ ] `npm run check`
- [ ] `npm run build`
- [ ] `cargo fmt --check` (in `src-tauri/`)
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`

## Version bump (if releasing)

All **five** must match: `package.json` + `package-lock.json` + `Cargo.toml` + `Cargo.lock` + `tauri.conf.json`.
