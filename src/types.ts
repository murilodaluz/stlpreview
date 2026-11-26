export type AppSettings = { lastRoot: string | null; orcaPath: string | null };

export type ModelKind = "stl" | "3mf";

export type FolderNode = {
  name: string;
  relPath: string;
  children: FolderNode[];
  fileCount: number;
  totalCount: number;
  isProject: boolean;
  previewPath: string | null;
  previewRenderedMs: number | null;
  customPreview: boolean;
};

export type ModelItem = {
  path: string;
  relPath: string;
  folderRelPath: string;
  name: string;
  kind: ModelKind;
  sizeBytes: number;
  modifiedMs: number;
  previewPath: string | null;
  previewRenderedMs: number | null;
  triangles: number | null;
  error: string | null;
  customPreview: boolean;
};

export type Library = {
  root: string;
  tree: FolderNode;
  items: ModelItem[];
  scannedAtMs: number | null;
};

export type ScanProgress = { done: number; total: number; current: string };
