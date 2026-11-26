import type { ReactElement } from "react";
import type { ProjectActions } from "../actions";
import type { FolderNode } from "../types";

type FolderHeaderProps = {
  node: FolderNode;
  includeSubfolders: boolean;
  actions: ProjectActions;
};

export function FolderHeader(props: FolderHeaderProps): ReactElement {
  const { node, actions } = props;
  const count: number = props.includeSubfolders ? node.totalCount : node.fileCount;
  const isRoot: boolean = node.relPath === "";
  const noun: string = node.isProject ? "part" : "file";

  return (
    <div className="folder-header">
      <div className="folder-title">
        <h2 title={node.relPath === "" ? node.name : node.relPath}>{node.name}</h2>
        {node.isProject ? <span className="badge">project</span> : null}
        <span className="folder-count">{count === 1 ? `1 ${noun}` : `${count} ${noun}s`}</span>
      </div>
      <div className="folder-actions">
        {node.isProject ? (
          <>
            <button
              type="button"
              className="btn btn-small btn-primary"
              onClick={(): void => void actions.onOpenAll(node)}
            >
              Open all in OrcaSlicer
            </button>
            <button
              type="button"
              className="btn btn-small"
              onClick={(): void => void actions.onChoosePreview(node)}
            >
              Choose preview
            </button>
            {node.customPreview ? (
              <button
                type="button"
                className="btn btn-small"
                onClick={(): void => void actions.onResetPreview(node)}
              >
                Reset preview
              </button>
            ) : null}
          </>
        ) : null}
        {!isRoot ? (
          <button
            type="button"
            className="btn btn-small"
            onClick={(): void => void actions.onSetProject(node, !node.isProject)}
          >
            {node.isProject ? "Unmark project" : "Mark as project"}
          </button>
        ) : null}
      </div>
    </div>
  );
}
