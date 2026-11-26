import { useState, type ChangeEvent, type ReactElement } from "react";
import { detectOrca, errorMessage } from "../api";
import { CACHE_FILE_NAME } from "../format";
import type { AppSettings } from "../types";

type SettingsPanelProps = {
  settings: AppSettings;
  root: string | null;
  onSave: (settings: AppSettings) => Promise<void>;
  onError: (message: string) => void;
};

export function SettingsPanel(props: SettingsPanelProps): ReactElement {
  const [orcaPath, setOrcaPath] = useState<string>(props.settings.orcaPath ?? "");
  const [detecting, setDetecting] = useState<boolean>(false);

  const detect = async (): Promise<void> => {
    setDetecting(true);
    try {
      const found: string | null = await detectOrca();
      if (found === null) {
        props.onError("OrcaSlicer was not found on this system");
      } else {
        setOrcaPath(found);
      }
    } catch (error: unknown) {
      props.onError(errorMessage(error));
    } finally {
      setDetecting(false);
    }
  };

  const save = async (): Promise<void> => {
    const trimmed: string = orcaPath.trim();
    await props.onSave({
      ...props.settings,
      orcaPath: trimmed === "" ? null : trimmed,
    } satisfies AppSettings);
  };

  return (
    <section className="settings">
      <label className="field">
        <span className="field-label">OrcaSlicer executable or launch command</span>
        <div className="field-row">
          <input
            type="text"
            value={orcaPath}
            placeholder="orca-slicer"
            onChange={(event: ChangeEvent<HTMLInputElement>): void =>
              setOrcaPath(event.target.value)
            }
          />
          <button type="button" className="btn" disabled={detecting} onClick={detect}>
            {detecting ? "Detecting..." : "Detect"}
          </button>
          <button type="button" className="btn btn-primary" onClick={save}>
            Save
          </button>
        </div>
      </label>
      <div className="field">
        <span className="field-label">Cache file</span>
        <code className="cache-path">
          {props.root !== null ? `${props.root}/${CACHE_FILE_NAME}` : "Choose a folder first"}
        </code>
      </div>
    </section>
  );
}
