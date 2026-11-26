import type { ChangeEvent, ReactElement } from "react";
import type { ScanProgress } from "../types";

type ToolbarProps = {
  root: string | null;
  scanning: boolean;
  progress: ScanProgress | null;
  query: string;
  settingsOpen: boolean;
  onChoose: () => void;
  onRefresh: () => void;
  onQueryChange: (query: string) => void;
  onToggleSettings: () => void;
};

export function Toolbar(props: ToolbarProps): ReactElement {
  const percent: number =
    props.progress !== null && props.progress.total > 0
      ? Math.round((props.progress.done / props.progress.total) * 100)
      : 0;

  return (
    <header className="toolbar">
      <div className="toolbar-row">
        <span className="brand">sltpreview</span>
        <span className="root-path" title={props.root ?? ""}>
          {props.root === null ? "No folder selected" : `\u200E${props.root}\u200E`}
        </span>
        <button type="button" className="btn" onClick={props.onChoose}>
          Choose folder
        </button>
        <button
          type="button"
          className="btn btn-primary"
          disabled={props.scanning || props.root === null}
          onClick={props.onRefresh}
        >
          {props.scanning
            ? props.progress !== null
              ? `Refreshing ${props.progress.done}/${props.progress.total}`
              : "Refreshing..."
            : "Refresh"}
        </button>
        <input
          type="search"
          className="search"
          placeholder="Search by file name"
          value={props.query}
          onChange={(event: ChangeEvent<HTMLInputElement>): void =>
            props.onQueryChange(event.target.value)
          }
        />
        <button
          type="button"
          className={props.settingsOpen ? "btn btn-icon active" : "btn btn-icon"}
          title="Settings"
          aria-label="Settings"
          aria-pressed={props.settingsOpen}
          onClick={props.onToggleSettings}
        >
          &#9881;
        </button>
      </div>
      {props.scanning ? (
        <div className="progress" role="progressbar" aria-valuenow={percent}>
          <div className="progress-bar" style={{ width: `${percent}%` }} />
        </div>
      ) : null}
    </header>
  );
}
