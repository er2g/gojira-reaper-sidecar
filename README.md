# Gojira Link

Hybrid REAPER sidecar project: native DLL extension + Tauri control UI + Rust protocol crates.

## Purpose

This repository provides a local bridge between REAPER and an external control brain, with a desktop UI for orchestration and diagnostics.

## Components

- `reaper_gojira_dll/`: REAPER extension DLL
- `gojira_brain_ui/`: Tauri-based desktop UI
- `gojira_protocol/`: shared protocol definitions
- `scripts/`, `tools/`: helper automation

## Quick Start (Windows)

1. Install Rust (`cargo`) and Node.js
2. Ensure REAPER is installed
3. Optionally create `.env` from `.env.example`
4. Run:

```bat
GOJIRA_START.cmd
```

Useful commands:

- `GOJIRA_DOCTOR.cmd`: environment checks
- `GOJIRA_BUILD_DLL.cmd`: build DLL only
- `GOJIRA_INSTALL_DLL.cmd`: build and copy DLL into REAPER UserPlugins
- `GOJIRA_UI_DEV.cmd`: run UI only

## Notes

Set `REAPER_USERPLUGINS_DIR` if your REAPER plugin path differs from default.
