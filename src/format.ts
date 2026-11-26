export function formatSize(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatTriangles(triangles: number | null): string | null {
  if (triangles === null) {
    return null;
  }
  return `${triangles.toLocaleString()} tris`;
}

export const CACHE_FILE_NAME: string = ".sltpreview-cache.json";
