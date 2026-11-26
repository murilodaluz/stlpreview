# sltpreview

Desktop previewer for `.stl` and `.3mf` files. Pick a folder, press Refresh, and every model gets a rendered PNG in an `img/` folder next to it. Browse them in a folder tree plus grid and open any file in OrcaSlicer.

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

## Features

- Folder tree plus preview grid, dark theme only.
- Mark any folder as a project: its parts collapse into one card with a composite preview in parent views, and "Open all in OrcaSlicer" sends every part at once. The marker is `<folder>/.sltproject.json`, the composite is `<folder>/img/_project.png`.
- Rename a model from its card. Previews and the cache follow the file, so nothing is re-rendered.
- Choose a custom preview image (PNG, JPG, WebP) for a model or a project. It is stored as `img/<stem>.custom.png` or `img/_project.custom.png`, is never overwritten by Refresh, and can be reset.

## How it works

- All scanning, parsing, rendering and caching happens in Rust (`src-tauri/src`). React is UI only.
- The cache lives at `<root>/.sltpreview-cache.json`. A file is re-rendered only when its size or mtime changed or its PNG is missing.
- Nothing is scanned on startup; the last folder is reopened from the cache. Refresh does the work.
- Folders named `img` and dot-folders are skipped.
- OrcaSlicer is auto-detected (PATH, flatpak, common AppImage locations) and can be overridden in Settings with a path or a launch command such as `flatpak run io.github.softfever.OrcaSlicer`.

## Fixtures

```
python3 -I scripts/make_fixtures.py
```

Regenerates `fixtures/library`, a sample root with nested folders, ASCII and binary STL, 3MF with transforms and components, and a corrupt file.

## Tests

```
cd src-tauri && cargo test
pnpm build
```
