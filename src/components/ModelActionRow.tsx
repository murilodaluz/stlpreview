import type { ReactElement } from "react";
import type { ModelActions } from "../actions";
import type { ModelItem } from "../types";

type ModelActionRowProps = {
  item: ModelItem;
  actions: ModelActions;
  large: boolean;
  disabled: boolean;
  onStartRename: () => void;
};

export function ModelActionRow(props: ModelActionRowProps): ReactElement {
  const { item, actions } = props;
  return (
    <div className="action-stack">
      <button
        type="button"
        className={props.large ? "btn btn-primary" : "btn btn-small btn-primary"}
        disabled={props.disabled}
        onClick={(): void => void actions.onOpen(item)}
      >
        Open in OrcaSlicer
      </button>
      <div className="link-row">
        <button
          type="button"
          className="btn-link"
          disabled={props.disabled}
          onClick={(): void => void actions.onReveal(item)}
        >
          Show in folder
        </button>
        <button
          type="button"
          className="btn-link"
          disabled={props.disabled}
          onClick={props.onStartRename}
        >
          Rename
        </button>
        <button
          type="button"
          className="btn-link"
          disabled={props.disabled}
          onClick={(): void => void actions.onChoosePreview(item)}
        >
          Choose preview
        </button>
        {item.customPreview ? (
          <button
            type="button"
            className="btn-link"
            disabled={props.disabled}
            onClick={(): void => void actions.onResetPreview(item)}
          >
            Reset preview
          </button>
        ) : null}
      </div>
    </div>
  );
}
