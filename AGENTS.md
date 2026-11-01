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
- The Settings and About windows are plain Win32 windows built by `src/ui.rs`, with layouts in 96-DPI pixels that are rescaled on `WM_DPICHANGED`. Open them outside `with_app`, because creating a window sends messages that need the app state. Checkboxes are owner-drawn so their labels can use dark-mode colors.
- `src/theme.rs` follows `AppsUseLightTheme` for windows and menus and `SystemUsesLightTheme` for the tray icon. The `theme` config key overrides the window and menu colors. Dark menus rely on the undocumented uxtheme ordinals 135 and 136.

## Release

Bump `version` in `Cargo.toml`, commit, build, then run `gh release create vX.Y.Z target/release/keepawake.exe`. Keep the asset name, because the README's install steps download `releases/latest/download/keepawake.exe`.

## Manual check

UI changes are checked in both themes. Back up `config.ini`, then set `theme=dark` or `theme=light`. Open windows switch within 5 s.


Activity mode is verified by watching the idle timer. Back up the user's `config.ini` first, set `mode=activity` and `interval_secs=5`, run the exe, and poll `GetLastInputInfo`. Idle time should drop back to near 0 about every 5 s. Restore the config afterwards.
