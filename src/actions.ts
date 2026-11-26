import type { FolderNode, ModelItem } from "./types";

export type ModelActions = {
  onPreview: (item: ModelItem) => void;
  onOpen: (item: ModelItem) => Promise<void>;
  onReveal: (item: ModelItem) => Promise<void>;
  onRename: (item: ModelItem, newName: string) => Promise<void>;
  onChoosePreview: (item: ModelItem) => Promise<void>;
  onResetPreview: (item: ModelItem) => Promise<void>;
};

export type ProjectActions = {
  onNavigate: (relPath: string) => void;
  onOpenAll: (node: FolderNode) => Promise<void>;
  onChoosePreview: (node: FolderNode) => Promise<void>;
  onResetPreview: (node: FolderNode) => Promise<void>;
  onSetProject: (node: FolderNode, isProject: boolean) => Promise<void>;
};
