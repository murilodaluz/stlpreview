import type { FolderNode, ModelItem } from "./types";

export type GridEntry =
  | { kind: "project"; node: FolderNode; partCount: number }
  | { kind: "model"; item: ModelItem };

export type GroupingInput = {
  tree: FolderNode;
  items: ModelItem[];
  selected: string;
  includeSubfolders: boolean;
  query: string;
};

export function isInside(relPath: string, folder: string): boolean {
  return folder === "" || relPath === folder || relPath.startsWith(`${folder}/`);
}

export function parentOf(relPath: string): string {
  const index: number = relPath.lastIndexOf("/");
  return index < 0 ? "" : relPath.slice(0, index);
}

export function ancestorsOf(relPath: string): string[] {
  const result: string[] = [""];
  if (relPath === "") {
    return result;
  }
  const segments: string[] = relPath.split("/");
  let current: string = "";
  for (const segment of segments.slice(0, -1)) {
    current = current === "" ? segment : `${current}/${segment}`;
    result.push(current);
  }
  return result;
}

export function flattenFolders(node: FolderNode): FolderNode[] {
  const result: FolderNode[] = [node];
  for (const child of node.children) {
    result.push(...flattenFolders(child));
  }
  return result;
}

export function findFolder(node: FolderNode, relPath: string): FolderNode | null {
  return (
    flattenFolders(node).find((candidate: FolderNode): boolean => candidate.relPath === relPath) ??
    null
  );
}

export function itemsUnder(items: ModelItem[], folder: string): ModelItem[] {
  return items
    .filter((item: ModelItem): boolean => isInside(item.folderRelPath, folder))
    .sort((a: ModelItem, b: ModelItem): number =>
      a.relPath < b.relPath ? -1 : a.relPath > b.relPath ? 1 : 0,
    );
}

function inSelection(item: ModelItem, selected: string, include: boolean): boolean {
  if (selected === "") {
    return include ? true : item.folderRelPath === "";
  }
  if (item.folderRelPath === selected) {
    return true;
  }
  return include && item.folderRelPath.startsWith(`${selected}/`);
}

export function buildGridEntries(input: GroupingInput): GridEntry[] {
  const needle: string = input.query.trim().toLowerCase();
  const searching: boolean = needle !== "";
  const matching: ModelItem[] = input.items.filter(
    (item: ModelItem): boolean => !searching || item.name.toLowerCase().includes(needle),
  );

  const relevant: FolderNode[] = flattenFolders(input.tree).filter((node: FolderNode): boolean => {
    if (!node.isProject || node.relPath === "" || node.relPath === input.selected) {
      return false;
    }
    if (isInside(input.selected, node.relPath)) {
      return false;
    }
    return input.includeSubfolders
      ? isInside(node.relPath, input.selected)
      : parentOf(node.relPath) === input.selected;
  });

  const shallowest: FolderNode[] = relevant.filter(
    (node: FolderNode): boolean =>
      !relevant.some(
        (other: FolderNode): boolean =>
          other.relPath !== node.relPath && isInside(node.relPath, other.relPath),
      ),
  );

  const projectEntries: GridEntry[] = shallowest
    .map(
      (node: FolderNode): GridEntry => ({
        kind: "project",
        node,
        partCount: matching.filter((item: ModelItem): boolean =>
          isInside(item.folderRelPath, node.relPath),
        ).length,
      }),
    )
    .filter((entry: GridEntry): boolean => !(searching && entry.kind === "project" && entry.partCount === 0))
    .sort((a: GridEntry, b: GridEntry): number =>
      a.kind === "project" && b.kind === "project" ? a.node.name.localeCompare(b.node.name) : 0,
    );

  const modelEntries: GridEntry[] = matching
    .filter(
      (item: ModelItem): boolean =>
        inSelection(item, input.selected, input.includeSubfolders) &&
        !shallowest.some((node: FolderNode): boolean => isInside(item.folderRelPath, node.relPath)),
    )
    .sort((a: ModelItem, b: ModelItem): number => a.name.localeCompare(b.name))
    .map((item: ModelItem): GridEntry => ({ kind: "model", item }));

  return [...projectEntries, ...modelEntries];
}
