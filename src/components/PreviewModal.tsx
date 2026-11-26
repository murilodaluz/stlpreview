import { useEffect, useState, type MouseEvent, type ReactElement } from "react";
import type { ModelActions } from "../actions";
import { previewSrcFor } from "../api";
import { formatSize, formatTriangles } from "../format";
import type { ModelItem } from "../types";
import { ModelActionRow } from "./ModelActionRow";
import { RenameInput } from "./RenameInput";

type PreviewModalProps = {
  item: ModelItem;
  actions: ModelActions;
  onClose: () => void;
};

export function PreviewModal(props: PreviewModalProps): ReactElement {
  const { onClose } = props;
  const [editing, setEditing] = useState<boolean>(false);
  const [pending, setPending] = useState<boolean>(false);
  const src: string | null = previewSrcFor(props.item.previewPath, props.item.previewRenderedMs);
  const triangles: string | null = formatTriangles(props.item.triangles);

  useEffect((): (() => void) => {
    const handler = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", handler);
    return (): void => window.removeEventListener("keydown", handler);
  }, [onClose]);

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
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        onClick={(event: MouseEvent<HTMLDivElement>): void => event.stopPropagation()}
      >
        <button
          type="button"
          className="btn btn-icon modal-close"
          aria-label="Close"
          onClick={onClose}
        >
          &times;
        </button>
        <div className="modal-image">
          {src !== null ? <img src={src} alt={props.item.name} /> : null}
          {props.item.customPreview ? <span className="corner-tag">custom</span> : null}
        </div>
        <div className="modal-info">
          {editing ? (
            <RenameInput
              initial={props.item.name}
              pending={pending}
              onConfirm={(name: string): void => void confirm(name)}
              onCancel={(): void => setEditing(false)}
            />
          ) : (
            <div className="modal-path" title={props.item.relPath}>
              {props.item.relPath}
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
            large
            disabled={pending}
            onStartRename={(): void => setEditing(true)}
          />
        </div>
      </div>
    </div>
  );
}
