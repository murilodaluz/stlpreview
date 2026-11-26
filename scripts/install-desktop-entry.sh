#!/usr/bin/env bash
set -euo pipefail

app_id="sltpreview"
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
binary="$repo_root/src-tauri/target/release/$app_id"
icons_src="$repo_root/src-tauri/icons"
svg_src="$repo_root/app-icon.svg"
data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
icons_dest="$data_home/icons/hicolor"
apps_dest="$data_home/applications"
desktop_file="$apps_dest/$app_id.desktop"

if [[ ! -x "$binary" ]]; then
  echo "error: $binary not found." >&2
  echo "Build it first with: pnpm tauri build --no-bundle" >&2
  exit 1
fi

if [[ "$binary" == *$'\n'* ]]; then
  echo "error: the repository path contains a newline and cannot be used in a desktop entry." >&2
  exit 1
fi

desktop_exec_arg() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//\`/\\\`}"
  value="${value//\$/\\\$}"
  value="${value//\\/\\\\}"
  value="${value//%/%%}"
  printf '"%s"' "$value"
}

icon_sources=("32x32.png" "64x64.png" "128x128.png" "128x128@2x.png" "icon.png")
icon_sizes=("32x32" "64x64" "128x128" "256x256" "512x512")

for i in "${!icon_sources[@]}"; do
  file="$icons_src/${icon_sources[$i]}"
  target="$icons_dest/${icon_sizes[$i]}/apps/$app_id.png"
  if [[ -f "$file" ]]; then
    install -Dm644 "$file" "$target"
    echo "installed $target"
  fi
done

install -Dm644 "$svg_src" "$icons_dest/scalable/apps/$app_id.svg"
echo "installed $icons_dest/scalable/apps/$app_id.svg"

mkdir -p "$apps_dest"
cat > "$desktop_file" <<EOF
[Desktop Entry]
Type=Application
Name=sltpreview
Comment=Preview STL and 3MF files
Exec=$(desktop_exec_arg "$binary")
Icon=$app_id
Terminal=false
Categories=Graphics;3DGraphics;Viewer;
StartupWMClass=$app_id
EOF
chmod 644 "$desktop_file"
echo "installed $desktop_file"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$apps_dest" || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$icons_dest" >/dev/null 2>&1 || true
fi
