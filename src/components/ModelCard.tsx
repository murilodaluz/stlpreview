import { useState, type ReactElement } from "react";
import type { ModelActions } from "../actions";
import { previewSrcFor } from "../api";
import { formatSize, formatTriangles } from "../format";
import type { ModelItem } from "../types";
import { ModelActionRow } from "./ModelActionRow";
import { RenameInput } from "./RenameInput";

type ModelCardProps = {
  item: ModelItem;
  actions: ModelActions;
};

export function ModelCard(props: ModelCardProps): ReactElement {
  const [editing, setEditing] = useState<boolean>(false);
  const [pending, setPending] = useState<boolean>(false);
  const src: string | null = previewSrcFor(props.item.previewPath, props.item.previewRenderedMs);
  const triangles: string | null = formatTriangles(props.item.triangles);

  const confirm = async (name: string): Promise<void> => {
    setPending(true);
    try {
      await props.actions.onRename(props.item, name);
    } finally {
      setPending(false);
      setEditing(false);
    }
  };

  return (
    <article className="card">
      <div
        className={src !== null ? "card-image clickable" : "card-image"}
        onClick={(): void => {
          if (src !== null) {
            props.actions.onPreview(props.item);
          }
        }}
      >
        {src !== null ? (
          <img src={src} alt={props.item.name} loading="lazy" />
        ) : (
          <div className="placeholder">
            <strong>No preview</strong>
            <span>{props.item.error ?? "Not rendered yet"}</span>
          </div>
        )}
        {props.item.customPreview ? <span className="corner-tag">custom</span> : null}
      </div>
      <div className="card-body">
        {editing ? (
          <RenameInput
            initial={props.item.name}
            pending={pending}
            onConfirm={(name: string): void => void confirm(name)}
            onCancel={(): void => setEditing(false)}
          />
        ) : (
          <div className="card-name" title={props.item.relPath}>
            {props.item.name}
          </div>
        )}
        <div className="card-meta">
          <span className="badge">{props.item.kind}</span>
          {triangles !== null ? <span>{triangles}</span> : null}
          <span>{formatSize(props.item.sizeBytes)}</span>
        </div>
        <ModelActionRow
          item={props.item}
          actions={props.actions}
          large={false}
          disabled={pending}
          onStartRename={(): void => setEditing(true)}
        />
      </div>
    </article>
  );
}
