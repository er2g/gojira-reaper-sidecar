# gojira-reaper-sidecar

## Quick Start (Windows)

Prereqs:
- Rust toolchain (`cargo`)
- Node.js + npm
- REAPER installed (the DLL is a REAPER extension plugin)

1) Create `.env` from `.env.example` (optional but recommended for the UI).
2) Double-click `GOJIRA_START.cmd`.

What it does:
- Builds `reaper_gojira_dll` (debug by default)
- Copies the DLL into `%APPDATA%\\REAPER\\UserPlugins`
- Starts the Tauri UI in dev mode

Optional:
- Set `REAPER_USERPLUGINS_DIR` if your REAPER UserPlugins folder is elsewhere.
- Use `GOJIRA_START.cmd -Release` for a release build.

Other shortcuts:
- `GOJIRA_DOCTOR.cmd` (prereq + path check)
- `GOJIRA_BUILD_DLL.cmd` (only build)
- `GOJIRA_INSTALL_DLL.cmd` (build + copy to REAPER)
- `GOJIRA_UI_DEV.cmd` (only start the UI)
