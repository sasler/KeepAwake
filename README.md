# KeepAwake

A tiny Windows tray app (~350 KB exe, ~1.5 MB private memory, near-zero CPU, no dependencies besides `windows-sys`) that stops Windows from locking or sleeping while you're idle.

It follows the Windows light/dark setting for its menu and its Settings and About windows, and uses a tray icon that stays visible on both light and dark taskbars.

The icons are drawn in code ([src/art.rs](src/art.rs)) at the exact size Windows needs, and `build.rs` embeds the same artwork as the exe icon.

## Install

1. Download `keepawake.exe` from the [latest release](https://github.com/sasler/KeepAwake/releases/latest).
2. Put it somewhere permanent, for example `%LOCALAPPDATA%\Programs\KeepAwake\`.
3. Run it. The exe isn't code-signed, so SmartScreen may warn the first time: choose **More info → Run anyway**.
4. Right-click the mug in the tray and turn on **Start with Windows**.

Or, from PowerShell:

```powershell
$dir = "$env:LOCALAPPDATA\Programs\KeepAwake"
New-Item -ItemType Directory -Force $dir | Out-Null
Invoke-WebRequest https://github.com/sasler/KeepAwake/releases/latest/download/keepawake.exe -OutFile "$dir\keepawake.exe"
Start-Process "$dir\keepawake.exe"
```

**Update:** choose **Exit** from the tray menu, replace the exe, and start it again. **About KeepAwake** shows the installed version, and its *Check for updates* button opens the latest release.

**Uninstall:** turn off *Start with Windows*, choose **Exit**, then delete the exe and `%APPDATA%\KeepAwake`.

## Modes

| Mode | Icon | What it does |
|---|---|---|
| Off | gray mug | Nothing. Windows behaves normally. |
| Keep screen awake | steaming amber mug | Asks Windows to keep the display and system on (`SetThreadExecutionState`). Stops screen-off, sleep and screensaver-based locks. No fake input. |
| Simulate activity | steaming mug with a green badge | Keep screen awake, plus a harmless input (F15 key, or a zero-distance mouse move) after `interval_secs` of idle time. Resets the idle timer, so it also stops idle-time locks and "Away" status in chat apps. Sends nothing while you're actively using the PC. |

## Use

- **Left-click** the tray icon: toggle between Off and the last used mode.
- **Right-click**: menu with mode, **Duration** (indefinitely, 30 min – 8 h, or until end of work hours; reverts to Off afterwards), **Work-hours schedule**, **Start with Windows**, **Settings…**, **About KeepAwake**, **Exit**.
- Hover for the current state and time left.

The mode and timer are remembered across restarts.

## Settings

**Settings…** opens a window for startup, appearance (same as Windows, light or dark), the simulated-activity interval and input, and the work-hours schedule.

Everything is stored in `%APPDATA%\KeepAwake\config.ini`. *Open config file* in the Settings window opens it in Notepad; saved edits apply within 5 seconds.

```ini
mode=off              # off | awake | activity
until=0               # unix time to switch back to off; 0 = indefinitely
interval_secs=60      # idle seconds before simulated input (5-3600)
input=f15             # f15 | mouse
schedule_enabled=false
schedule_days=Mon-Fri # or Mon,Wed,Fri
schedule_start=08:00
schedule_end=17:00
schedule_mode=activity
theme=system          # system | light | dark
```

The schedule applies only while the manual mode is **Off**: during the window it uses `schedule_mode`, outside it does nothing. Overnight windows (e.g. 22:00–06:00) work.

**Start with Windows** writes a per-user `HKCU\...\Run` entry (no admin needed). If you move the exe, toggle it off and on again.

## Build

Needs Rust (MSVC toolchain).

```bash
cargo build --release
```

Output: `target\release\keepawake.exe`. Copy it anywhere, run it, and enable *Start with Windows* from the menu.

The version comes from `Cargo.toml`. `build.rs` adds the git commit and build date (shown in About) and embeds Windows version info, which shows up under the exe's Properties → Details.

## Release

Bump `version` in `Cargo.toml`, commit, then build and publish:

```bash
cargo build --release
gh release create vX.Y.Z target/release/keepawake.exe --title "KeepAwake X.Y.Z" --generate-notes
```

Keep the asset name `keepawake.exe`: the install steps download `releases/latest/download/keepawake.exe`.

## Limits

- Windows **Dynamic Lock** (locks when your phone's Bluetooth goes away) and **presence sensing** (locks when a laptop sensor sees you leave) don't use the idle timer, so this app can't stop them. Turn them off in Settings → Accounts → Sign-in options / Privacy & security → Presence sensing.
- If your organization requires the screen lock on this device, check that this is allowed before using it.

## License

[MIT](LICENSE)
