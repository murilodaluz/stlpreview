import type { ReactElement } from "react";
import type { ProjectActions } from "../actions";
import { previewSrcFor } from "../api";
import type { FolderNode } from "../types";

type ProjectCardProps = {
  node: FolderNode;
  partCount: number;
  actions: ProjectActions;
};

export function ProjectCard(props: ProjectCardProps): ReactElement {
  const src: string | null = previewSrcFor(props.node.previewPath, props.node.previewRenderedMs);
  const navigate = (): void => props.actions.onNavigate(props.node.relPath);

  return (
    <article className="card project-card">
      <div className="card-image clickable" onClick={navigate}>
        {src !== null ? (
          <img src={src} alt={props.node.name} loading="lazy" />
        ) : (
          <div className="placeholder">
            <strong>No preview</strong>
            <span>Refresh to build one</span>
          </div>
        )}
        {props.node.customPreview ? <span className="corner-tag">custom</span> : null}
      </div>
      <div className="card-body">
        <div className="card-name card-name-link" title={props.node.relPath} onClick={navigate}>
          {props.node.name}
        </div>
        <div className="card-meta">
          <span className="badge">project</span>
          <span>{props.partCount === 1 ? "1 part" : `${props.partCount} parts`}</span>
        </div>
        <div className="action-stack">
          <button
            type="button"
            className="btn btn-small btn-primary"
            onClick={(): void => void props.actions.onOpenAll(props.node)}
          >
            Open all in OrcaSlicer
          </button>
          <div className="link-row">
            <button
              type="button"
              className="btn-link"
              onClick={(): void => void props.actions.onChoosePreview(props.node)}
            >
              Choose preview
            </button>
          </div>
        </div>
      </div>
    </article>
  );
}
