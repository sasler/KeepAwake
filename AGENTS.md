# AGENTS.md

Windows-only tray app in Rust. See README.md for what it does.

## Build and test

- Needs the MSVC toolchain (`x86_64-pc-windows-msvc`); `build.rs` uses the Windows SDK resource compiler.
- `cargo build --release` → `target\release\keepawake.exe`
- `cargo test -- --include-ignored` runs the unit tests and writes icon previews to `target/preview/*.ico`.
- Only one instance can run (named mutex). Stop the running one before testing a new build.

## Constraints

- Keep it tiny: `windows-sys` is the only runtime dependency, with no async runtime, GUI framework or serde. Check the release exe size and private memory after changes.
- `src/art.rs` is compiled twice: into the app (tray icons) and into `build.rs` via `#[path]` (exe icon). It must stay plain `std` Rust with no crate imports.
- All UI runs on one thread. App state lives in a thread-local `RefCell` reached through `with_app`, which uses `try_borrow_mut` because `TrackPopupMenu` re-enters the window procedure. Never hold a borrow across a modal call.
- Settings live in `%APPDATA%\KeepAwake\config.ini`, and the app reloads the file when it changes on disk. New keys need a default, parsing in `Config::load` and a line in the `Config::save` template.

## Manual check

Activity mode is verified by watching the idle timer. Back up the user's `config.ini` first, set `mode=activity` and `interval_secs=5`, run the exe, and poll `GetLastInputInfo`. Idle time should drop back to near 0 about every 5 s. Restore the config afterwards.
