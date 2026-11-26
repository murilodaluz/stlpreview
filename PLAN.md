# sltpreview — plan and contract

Single-screen Tauri 2 desktop app (Linux first). User picks a root folder; the app scans it for `.stl` and `.3mf` files, renders a PNG preview of each into an `img/` folder next to the file, keeps a cache so only new or changed files are re-rendered, and shows everything in a sidebar-tree + grid layout with a button to open a file in OrcaSlicer.

Project root: `/home/saruman/orodruin/randomprojects/sltpreview`
Stack: Tauri 2, Rust (edition 2024), React 19 + TypeScript + Vite, pnpm. No UI library, plain CSS.

## Hard rules (apply to Rust and TypeScript)

- All heavy work happens in Rust: directory walking, STL/3MF parsing, rendering, cache diffing, cache IO, settings IO, OrcaSlicer detection and launch. The React side is UI only: it invokes commands, listens to events, filters an already-loaded item list by name, and renders. No file IO, no parsing, no image processing in TypeScript.
- No code comments of any kind (no doc comments, no `//`, no section headers). Only compiler/linter directives like `#[allow(...)]`, `// @ts-expect-error`, `// eslint-disable-next-line` are allowed.
- TypeScript: every variable, parameter and return value has a declared type. No `any`. No lying `as` casts; use `satisfies`, type guards, discriminated unions.
- Rust: no `unwrap()` on fallible IO; commands return `Result<T, String>`.
- Do not add git, CI, or AI attribution anywhere.

## Behaviour

1. Startup: load settings from the app config dir. If `lastRoot` is set, call `load_library` (cache only, no scan, no rendering) and show it.
2. "Choose folder" opens a native directory picker, saves `lastRoot`, then calls `load_library` (cache only).
3. "Refresh" calls `refresh_library`: walks the root, renders previews for files that are new, changed (size or mtime differ from cache), or whose preview PNG is missing; drops cache entries for files that vanished; writes the cache; returns the full library. Progress is emitted as Tauri events.
4. Preview PNG location: `<dir of file>/img/<stem>.png`. If two model files in the same folder share a stem (e.g. `cube.stl` and `cube.3mf`) each uses `<stem>.<ext>.png` instead. `img/` folders and dot-folders are skipped while walking.
5. Cache file: `<root>/.sltpreview-cache.json` (format below).
6. "Open in OrcaSlicer": launches the configured OrcaSlicer with the file path as argument, detached. Settings panel lets the user set the executable path or run auto-detect.
7. The folder tree in the sidebar is built from the directories on disk (cheap walk, directories only, skipping `img/` and dot-folders). Item cards come only from the cache until Refresh is pressed.

## Rust commands (src-tauri)

All invoked from the frontend with `invoke<T>(name, args)`. Field names are camelCase on the wire (`#[serde(rename_all = "camelCase")]`).

```
get_settings() -> AppSettings
save_settings(settings: AppSettings) -> ()
detect_orca() -> Option<String>                      // launch spec or null
load_library(root: String) -> Library                // cache only
refresh_library(root: String) -> Library             // scan + render + cache write
open_in_orca(path: String) -> ()                     // uses settings.orcaPath; error if unset/not found
```

Events emitted during `refresh_library` on the app handle:

```
"scan-progress"  payload ScanProgress { done: u32, total: u32, current: String }   // current = relPath being rendered
```

## Wire types (TypeScript shape, Rust mirrors with serde)

```ts
type AppSettings = { lastRoot: string | null; orcaPath: string | null };

type ModelKind = "stl" | "3mf";

type FolderNode = {
  name: string;          // directory name; root uses its basename
  relPath: string;       // "" for root, otherwise "a/b" with forward slashes
  children: FolderNode[];
  fileCount: number;     // cached items directly in this folder
  totalCount: number;    // cached items in this folder and all descendants
};

type ModelItem = {
  path: string;          // absolute
  relPath: string;       // relative to root, forward slashes
  folderRelPath: string; // relPath of containing folder, "" for root
  name: string;          // file name with extension
  kind: ModelKind;
  sizeBytes: number;
  modifiedMs: number;
  previewPath: string | null;     // absolute path of PNG, null if render failed
  previewRenderedMs: number | null;
  triangles: number | null;
  error: string | null;
};

type Library = {
  root: string;
  tree: FolderNode;
  items: ModelItem[];
  scannedAtMs: number | null;
};

type ScanProgress = { done: number; total: number; current: string };
```

## Cache file format `<root>/.sltpreview-cache.json`

```json
{
  "version": 1,
  "scannedAtMs": 1730000000000,
  "entries": {
    "toys/cube.stl": {
      "sizeBytes": 684,
      "modifiedMs": 1729999999000,
      "previewRel": "toys/img/cube.png",
      "previewRenderedMs": 1730000000000,
      "triangles": 12,
      "error": null
    }
  }
}
```

## Settings file

`<app_config_dir>/settings.json` via `app.path().app_config_dir()`, same shape as `AppSettings`.

## OrcaSlicer detection and launch (`orca.rs`)

`detect_orca` returns the first that exists, as a launch spec string:
- `orca-slicer`, `OrcaSlicer`, `orcaslicer` found on PATH → that absolute path
- flatpak app `io.github.softfever.OrcaSlicer` installed (`flatpak info io.github.softfever.OrcaSlicer` exits 0) → `flatpak run io.github.softfever.OrcaSlicer`
- any file matching `~/Applications/OrcaSlicer*.AppImage`, `~/.local/bin/OrcaSlicer*.AppImage`, `~/Downloads/OrcaSlicer*.AppImage`, `/opt/OrcaSlicer*/bin/orca-slicer` → that path

`open_in_orca`: split `orcaPath` on whitespace into program + leading args (so `flatpak run io.github...` works), append the file path, spawn detached (`std::process::Command::spawn`, do not wait). If `orcaPath` is null, try `detect_orca` once and persist it; if still null, return an error string the UI can show.

## Mesh loading (`mesh.rs`)

`struct Mesh { triangles: Vec<[[f32; 3]; 3]> }` and `fn load_mesh(path) -> Result<Mesh, String>`.

STL: detect binary vs ASCII. Binary if file length == 84 + 50 * triangle_count_from_header; otherwise if it starts with `solid` treat as ASCII; otherwise binary. Ignore normals from the file, compute face normals from vertices.

3MF: zip archive. Find the model part: parse `_rels/.rels` for the relationship with type ending `3dmodel`, fallback `3D/3dmodel.model`. Parse with quick-xml:
- `<object id="..">` containing `<mesh><vertices><vertex x y z/></vertices><triangles><triangle v1 v2 v3/></triangles></mesh>` or `<components><component objectid=".." transform=".." p:path="/3D/Objects/x.model"/></components>`.
- `<build><item objectid=".." transform=".."/></build>`. Transform is 12 floats `m00 m01 m02 m10 m11 m12 m20 m21 m22 m30 m31 m32`: point' = p * M3x3 + translation (row-vector convention).
- Components with `p:path` refer to objects in another `.model` part inside the same zip; load those lazily. Objects of type `other` or `support` are skipped.
- If the result has zero triangles but the zip contains `Metadata/thumbnail.png` or `Metadata/plate_1.png`, copy that PNG as the preview instead and report `triangles: 0`.

## Rendering (`render.rs`)

Pure software rasterizer, no GPU, no external renderer.
- Output 512×512 RGBA PNG with transparent background. Render at 1024×1024 and box-downsample 2× for anti-aliasing.
- Orthographic camera. Model Z is up. View from azimuth 45° around Z and elevation 30° above the XY plane (a standard three-quarter view). Fit the bounding box into the frame with 8% margin; center on the bbox center.
- Z-buffer, flat shading per face. Light direction: from camera side, upper-left. Color = lerp(dark, light, 0.25 + 0.75 * max(0, n·l)) where dark = #d74925 and light = #febda1. Faces are two-sided (use abs of n·l when the normal faces away).
- Use the `png` crate (or `image` with only the png feature) for encoding.
- Per file: load mesh → render → create `img/` dir → write PNG atomically (write to temp then rename).
- Scan renders files in parallel with rayon; progress counter is an AtomicU32 and `scan-progress` is emitted after each file.

## Scan (`scan.rs`, `cache.rs`, `library.rs`)

- Walk with `walkdir`, skip dirs named `img` and names starting with `.`. Match extensions case-insensitively.
- An entry is stale when size or mtime differ from the cache, or the preview PNG does not exist.
- `load_library` never renders; it builds the tree from directories on disk and items from the cache (skipping cache entries whose source file no longer exists).
- Both commands return the same `Library` shape; `refresh_library` also writes the cache.

## Tauri config

- `tauri.conf.json`: `app.security.assetProtocol = { enable: true, scope: ["**"] }`, `csp` = `"default-src 'self'; img-src 'self' asset: http://asset.localhost data:; style-src 'self' 'unsafe-inline'"`. Window title `sltpreview`, 1280×820, min 900×600.
- Plugins: `tauri-plugin-dialog`, `tauri-plugin-opener`. Capabilities: `core:default`, `dialog:default`, `opener:default`, `opener:allow-reveal-item-in-dir`.
- Frontend loads preview images with `convertFileSrc(previewPath) + "?v=" + previewRenderedMs`.

## Frontend (src/)

Files: `src/types.ts` (wire types above), `src/api.ts` (typed wrappers over `invoke` and `listen`), `src/App.tsx`, components under `src/components/` (`Toolbar`, `FolderTree`, `ModelGrid`, `ModelCard`, `SettingsPanel`, `PreviewModal`), `src/styles.css`.

Layout: top toolbar; below it a left sidebar (folder tree, resizable not required, ~260px) and a main grid area.

Toolbar: app name, current root path (truncated, title attr shows full), buttons: Choose folder, Refresh (disabled while scanning, shows `done/total` and a thin progress bar), search input (filters by file name, case-insensitive), settings gear toggling the settings panel.

Sidebar: collapsible tree, root node expanded by default, each node shows name and `totalCount`. Selected node highlighted. Toggle "Include subfolders" (default on).

Grid: responsive CSS grid (`repeat(auto-fill, minmax(180px, 1fr))`). Card: square image area (object-fit contain, placeholder for null preview showing the error), file name (ellipsis, title attr), small meta line with kind badge, triangle count, size. Buttons on the card: "Open in OrcaSlicer" and "Show in folder" (opener `revealItemInDir`). Clicking the image opens `PreviewModal` with the image large, full relPath and the same buttons; Escape closes.

Settings panel: OrcaSlicer executable/launch command text field, "Detect" button (calls `detect_orca`, fills field), "Save". Also shows the cache file location for the current root.

Empty states: no root selected → centered prompt with Choose folder button; root selected but zero items → "No previews yet. Press Refresh to scan this folder."; filtered to zero → "Nothing matches".

Errors from `invoke` are shown in a dismissible toast bar at the top of the main area.

## Visual design

Light theme, warm neutrals, orange accents. CSS variables in `styles.css`:

```
--orange: #d74925;      primary buttons, selected tree node bar, progress bar, badges
--peach:  #febda1;      hover fills, selected row background, badge background
--orange-dark: #b63a1b; primary button hover
--bg: #fbf7f4;  --surface: #ffffff;  --border: #ead9cf;
--text: #2b211d; --muted: #7a6a62;
```

System font stack. Cards have 1px border, 10px radius, subtle shadow on hover. Keep everything in one CSS file.

## Fixtures

`fixtures/library/` is a sample root for manual and automated testing with nested folders and a mix of ASCII STL, binary STL and 3MF (with transforms and components). Generated by `scripts/make_fixtures.py`.

## Verification

- `cd src-tauri && cargo build` and `cargo test` pass.
- `pnpm build` passes (tsc strict).
- `pnpm tauri dev` opens the window; choosing `fixtures/library` and pressing Refresh produces `img/*.png` next to every fixture file.

---

# Phase 2 — projects, rename, custom previews, dark mode

Everything in phase 1 stays valid unless changed here. Same hard rules: no comments, full types, no `any`, heavy work in Rust, UI only in React.

## Feature summary

1. **Projects.** Any folder can be marked as a project. In a parent folder's grid, the parts inside a project folder are replaced by one project card with a composite preview. Selecting the project folder itself shows the individual parts plus a project header. A project has "Open all in OrcaSlicer".
2. **Rename.** A model file can be renamed from its card. Rust renames the file, moves its rendered and custom previews, and rewrites the cache entry.
3. **Custom preview.** The user can pick any PNG/JPG/WebP image for a model or a project. Rust converts it to a 512×512 PNG stored under a distinct name, so Refresh never overwrites it. It can be reset to the rendered one.
4. **Dark mode only.** One dark theme, no toggle.

## On-disk layout

```
<folder>/img/<stem>.png               rendered preview (unchanged rule; <stem>.<ext>.png on stem clash)
<folder>/img/<stem>.custom.png        custom preview for a model (same stem rule → <stem>.<ext>.custom.png)
<folder>/.sltproject.json             project marker: {"version":1}   (presence = folder is a project)
<folder>/img/_project.png             composite preview of the project folder
<folder>/img/_project.custom.png      custom preview of the project
```

Effective preview of a model = custom if it exists, else rendered. Same for projects. Files starting with `_project` are never treated as models. `.sltproject.json` is read even though dot-folders are skipped (it is a file, not a folder).

## Composite preview

Built in Rust from the effective previews of the project's parts (all model files in the project folder and its subfolders, in relPath order), using the `image` crate (features `png`, `jpeg`, `webp`, default-features off). Canvas 512×512 transparent. Tile layout by part count: 1 → 1×1, 2 → 2×1 (two tiles side by side, vertically centred), 3–4 → 2×2, 5–9 → 3×3, more than 9 → first 9. Each tile is the part preview resized to fit the cell with 6 px gutter, nearest-neighbour or triangle filter. Parts without any preview are skipped; a project with zero usable previews has `previewPath: null`.

Composites are rebuilt at the end of every `refresh_library`, and immediately by `set_project`, `set_custom_preview` and `rename_model` for any project affected. Rebuilding is cheap; always rebuild rather than tracking staleness.

## Custom preview conversion

`set_custom_preview` decodes the chosen image with the `image` crate, fits it into 512×512 preserving aspect ratio (transparent padding), encodes PNG, writes atomically to the `.custom.png` path. Passing `imagePath: null` deletes the custom file. Unsupported or unreadable images return an error string.

## Rename rules

`rename_model(root, relPath, newName)`:
- `newName` must be a single file name: non-empty, no `/` or `\`, not `.` or `..`. If it has no `.stl`/`.3mf` extension (case-insensitive), the original extension is appended. If the extension changes between stl and 3mf, return an error.
- Error if a file with the new name already exists.
- Renames the model file, then moves `img/<old>.png` and `img/<old>.custom.png` to the new preview names when they exist (recompute stem-clash naming for the folder after the rename, and also fix the sibling that may have lost or gained a clash).
- Rewrites the cache entry under the new relPath with the same size/mtime/triangles, so Refresh does not re-render.
- Returns the full `Library`.

## Wire type changes

```ts
type FolderNode = {
  name: string;
  relPath: string;
  children: FolderNode[];
  fileCount: number;
  totalCount: number;
  isProject: boolean;                   // new
  previewPath: string | null;           // new: effective composite/custom preview, absolute; null when not a project or none
  previewRenderedMs: number | null;     // new: mtime of that file
  customPreview: boolean;               // new
};

type ModelItem = {
  ...unchanged fields...
  customPreview: boolean;               // new; previewPath already points at the effective preview
};
```

Cache file gains nothing project-related (projects are marker files). The cache entry gains `customPreviewRel: string | null` for bookkeeping, but existence on disk is the source of truth: `load_library` and `refresh_library` check `img/<stem>.custom.png` on disk.

## New commands

```
rename_model(root: String, relPath: String, newName: String) -> Library
set_project(root: String, folderRelPath: String, isProject: bool) -> Library
set_custom_preview(root: String, relPath: String, isProject: bool, imagePath: Option<String>) -> Library
open_many_in_orca(paths: Vec<String>) -> ()        // one OrcaSlicer process with all paths as arguments
```

`set_project` with `isProject: true` writes the marker and builds the composite; with `false` deletes the marker, `img/_project.png` and `img/_project.custom.png`. The root folder (`folderRelPath == ""`) cannot be a project; return an error.

For `set_custom_preview` with `isProject: true`, `relPath` is the folder relPath; with `false` it is the model relPath.

## Frontend behaviour

**Grouping in the grid.** Given selected folder `S` and include-subfolders on, compute the set of project folders `P` from the tree. For each item: if it lies inside a project folder `Q` (its `folderRelPath` equals `Q` or starts with `Q + "/"`) and `Q !== S` and `Q` is not an ancestor of `S`, the item is hidden and a single project card for `Q` is shown instead (first matching project when nested; use the shallowest). Project cards come first in the grid, then models, each sorted by name. When `S` itself is a project, show its parts normally with a project header above the grid. With include-subfolders off, project folders directly under `S` still appear as project cards (that is the whole point of grouping); only deeper non-project content is hidden.

**Folder header** above the grid, always shown when a folder is selected: folder name, part/file count, and actions: "Mark as project" / "Unmark project" (not for root), and for projects: "Open all in OrcaSlicer", "Choose preview", "Reset preview" (only when `customPreview`).

**Project card**: composite image, folder name, "N parts" and a small PROJECT badge; click navigates to that folder (selects the node and expands ancestors). Buttons: "Open all in OrcaSlicer", "Choose preview".

**Model card**: existing buttons plus a small "Rename" action (pencil icon or text). Rename turns the name into an input prefilled with the current name; Enter confirms, Escape cancels, blur cancels. Also a "Choose preview" action and "Reset preview" when `customPreview` is true. A tiny "custom" tag is shown on the image corner when `customPreview`. The model card actions can live in an overflow row; keep the card uncluttered: primary "Open in OrcaSlicer", then a row of small text buttons: Show in folder · Rename · Preview ▾ (Choose / Reset).

**Preview modal** mirrors the same actions and shows `customPreview` state.

**Image picker**: `open({ multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }] })` from the dialog plugin.

All mutations (`renameModel`, `setProject`, `setCustomPreview`) return a `Library`; replace state with it. Keep the selected folder if it still exists; if a rename changed nothing structural, selection stays.

## Dark theme (only theme)

```
--orange: #d74925;        primary buttons, progress, selected tree bar, badges
--peach:  #febda1;        accent text, hover text, badge text on dark
--orange-dark: #b63a1b;   primary hover
--bg: #141110;            window background
--surface: #1d1816;       cards, sidebar, toolbar
--surface-2: #272020;     hover fills, inputs, image area background
--border: #352a26;
--text: #f3ebe6;
--muted: #a3928a;
--danger: #ff6b4a;        error text on dark
```

Previews are transparent PNGs; the card image area uses `--surface-2` so the orange models read well. Keep contrast AA for text. Scrollbars dark. No light theme remains anywhere (remove any `prefers-color-scheme` handling).
