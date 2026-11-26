import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { AppSettings, Library, ModelItem, ScanProgress } from "./types";

export function isString(value: unknown): value is string {
  return typeof value === "string";
}

export function isErrorLike(value: unknown): value is { message: string } {
  return (
    typeof value === "object" &&
    value !== null &&
    "message" in value &&
    isString(value.message)
  );
}

export function errorMessage(error: unknown): string {
  if (isString(error)) {
    return error;
  }
  if (isErrorLike(error)) {
    return error.message;
  }
  return "Unexpected error";
}

export function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export function saveSettings(settings: AppSettings): Promise<void> {
  return invoke<void>("save_settings", { settings });
}

export function detectOrca(): Promise<string | null> {
  return invoke<string | null>("detect_orca");
}

export function loadLibrary(root: string): Promise<Library> {
  return invoke<Library>("load_library", { root });
}

export function refreshLibrary(root: string): Promise<Library> {
  return invoke<Library>("refresh_library", { root });
}

export function openInOrca(path: string): Promise<void> {
  return invoke<void>("open_in_orca", { path });
}

export function listenScanProgress(
  handler: (progress: ScanProgress) => void,
): Promise<UnlistenFn> {
  return listen<ScanProgress>("scan-progress", (event: Event<ScanProgress>): void => {
    handler(event.payload);
  });
}

export async function pickFolder(): Promise<string | null> {
  const selected: string | null = await open({ directory: true, multiple: false });
  return selected;
}

export function revealInFolder(path: string): Promise<void> {
  return revealItemInDir(path);
}

export function previewSrcFor(path: string | null, renderedMs: number | null): string | null {
  if (path === null) {
    return null;
  }
  const version: number = renderedMs ?? 0;
  return `${convertFileSrc(path)}?v=${version}`;
}

export function previewSrc(item: ModelItem): string | null {
  return previewSrcFor(item.previewPath, item.previewRenderedMs);
}

export function renameModel(root: string, relPath: string, newName: string): Promise<Library> {
  return invoke<Library>("rename_model", { root, relPath, newName });
}

export function setProject(
  root: string,
  folderRelPath: string,
  isProject: boolean,
): Promise<Library> {
  return invoke<Library>("set_project", { root, folderRelPath, isProject });
}

export function setCustomPreview(
  root: string,
  relPath: string,
  isProject: boolean,
  imagePath: string | null,
): Promise<Library> {
  return invoke<Library>("set_custom_preview", { root, relPath, isProject, imagePath });
}

export function openManyInOrca(paths: string[]): Promise<void> {
  return invoke<void>("open_many_in_orca", { paths });
}

export async function pickImage(): Promise<string | null> {
  const selected: string | null = await open({
    multiple: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
  return selected;
}
