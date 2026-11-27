<p align="center">
  <img src="app-icon.svg" width="96" alt="">
</p>

# sltpreview

Desktop previewer for `.stl` and `.3mf` files, built with Tauri 2. Pick a folder, press Refresh, and every model gets a rendered PNG in an `img/` folder next to it. Browse them in a folder tree plus grid and open any file in OrcaSlicer. Linux first.

## Features

- Folder tree with per-folder model counts, plus a preview grid. An "Include subfolders" toggle controls whether the grid shows only the selected folder or everything below it.
- Name filter in the toolbar (case-insensitive match on the file name).
- Incremental Refresh with a progress bar. Only files that are new, changed (size or mtime), or missing their PNG are re-rendered; files that disappeared are dropped from the cache.
- Previews are rendered in software (no GPU) as 512x512 transparent PNGs and written to `img/<stem>.png` next to each model. When two models in a folder share a stem (`cube.stl`, `cube.3mf`), each gets `img/<stem>.<ext>.png`.
- STL in ASCII and binary form. 3MF with build-item and component transforms, including components stored in separate model parts. A 3MF with no mesh falls back to its embedded thumbnail.
- Cards show the file kind, triangle count and size. Clicking a preview opens an enlarged view with the full relative path and the same actions; Escape closes it.
- "Show in folder" reveals a model in the system file manager.
- Projects: mark any folder except the root as a project. Its parts collapse into one card with a composite preview in parent views, and "Open all in OrcaSlicer" sends every part at once. The marker is `<folder>/.sltproject.json`, the composite is `<folder>/img/_project.png`.
- Rename a model from its card or the enlarged view. Previews and the cache follow the file, so nothing is re-rendered.
- Custom preview image (PNG, JPG, WebP) for a model or a project. It is stored as `img/<stem>.custom.png` or `img/_project.custom.png`, is never overwritten by Refresh, and can be reset.
- Open in OrcaSlicer. The executable is auto-detected and can be overridden in Settings.
- The last folder is reopened from its cache on startup, without scanning.
- Dark theme only.

## Install

Download a package from the Releases page. None of them need Node, pnpm or Rust.

- `sltpreview_<version>_amd64.AppImage`: any x86_64 Linux with glibc 2.35 or newer. `chmod +x` it and run it. It does not add a launcher entry on its own.
- `sltpreview_<version>_amd64.deb`: Debian and Ubuntu. `sudo apt install ./sltpreview_<version>_amd64.deb` also installs the launcher entry and icon.
- `sltpreview_<version>_x64-setup.exe`: Windows installer.

The Windows build compiles but has not been run. OrcaSlicer auto-detection only knows Linux locations, so on Windows set its path in Settings.

## Build requirements

- Rust 1.88 or newer (edition 2024, let chains).
- Node.js `^20.19.0 || >=22.12.0` (required by Vite 8) and pnpm.
- The Tauri 2 system dependencies for your platform: see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).
- OrcaSlicer, optional, only for the "Open in OrcaSlicer" actions.

## Run

```
pnpm install
pnpm tauri dev
```

Release binary:

```
pnpm tauri build --no-bundle
./src-tauri/target/release/sltpreview
```

## App icon

The source is `app-icon.svg`. `pnpm icons` regenerates `src-tauri/icons` from it with `tauri icon`. The Android and iOS sets it also produces are git-ignored.

`pnpm install:desktop` installs a launcher entry for the current user, pointing at the release binary (build it first). It writes:

- `~/.local/share/applications/sltpreview.desktop`
- `~/.local/share/icons/hicolor/{32x32,64x64,128x128,256x256,512x512}/apps/sltpreview.png`
- `~/.local/share/icons/hicolor/scalable/apps/sltpreview.svg`

`$XDG_DATA_HOME` replaces `~/.local/share` when set. The desktop and icon caches are refreshed if `update-desktop-database` and `gtk-update-icon-cache` are installed. The entry is named `sltpreview` because GTK derives the window's Wayland app id and X11 `WM_CLASS` from the binary name; compositors use that match to show the icon.

## How it works

- All scanning, parsing, rendering and caching happens in Rust (`src-tauri/src`). React is UI only.
- The cache lives at `<root>/.sltpreview-cache.json`; its location is shown in Settings. A file is re-rendered only when its size or mtime changed or its PNG is missing.
- Nothing is scanned on startup; the last folder is reopened from the cache. Refresh does the work.
- Folders named `img` and dot-folders are skipped.
- Settings are stored in `settings.json` in the app config directory.
- OrcaSlicer is auto-detected in this order: `orca-slicer`, `OrcaSlicer` or `orcaslicer` on `PATH`; the flatpak `io.github.softfever.OrcaSlicer`; an OrcaSlicer AppImage in the home directory or in `~/Applications`, `~/Apps`, `~/AppImages`, `~/.local/bin`, `~/bin`, `~/Downloads` or `~/Desktop`; `/opt/OrcaSlicer*/bin/orca-slicer`. Settings accepts a path or a launch command such as `flatpak run io.github.softfever.OrcaSlicer`.

## Project layout

```
app-icon.svg                    icon source
src/                            React UI (App.tsx, components/, typed command wrappers in api.ts)
src-tauri/src/                  Rust backend: scan, mesh loading, rendering, cache, projects, rename, OrcaSlicer
src-tauri/icons/                icons generated from app-icon.svg
scripts/make_fixtures.py        sample library generator
scripts/install-desktop-entry.sh  per-user launcher entry and icons
.github/workflows/release.yml   release packages built on GitHub Actions
PLAN.md                         design notes and command contract
```

## Fixtures

```
python3 -I scripts/make_fixtures.py
```

Regenerates `fixtures/library`, a sample root with nested folders, ASCII and binary STL, 3MF with transforms and components, and a corrupt file. `fixtures/` is git-ignored.

## Tests

```
cd src-tauri && cargo test
pnpm build
```
