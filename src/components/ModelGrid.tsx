import type { ReactElement } from "react";
import type { ModelActions, ProjectActions } from "../actions";
import type { GridEntry } from "../grouping";
import { ModelCard } from "./ModelCard";
import { ProjectCard } from "./ProjectCard";

type ModelGridProps = {
  entries: GridEntry[];
  modelActions: ModelActions;
  projectActions: ProjectActions;
};

export function ModelGrid(props: ModelGridProps): ReactElement {
  return (
    <div className="grid">
      {props.entries.map((entry: GridEntry): ReactElement => {
        if (entry.kind === "project") {
          return (
            <ProjectCard
              key={`project:${entry.node.relPath}`}
              node={entry.node}
              partCount={entry.partCount}
              actions={props.projectActions}
            />
          );
        }
        return <ModelCard key={entry.item.path} item={entry.item} actions={props.modelActions} />;
      })}
    </div>
  );
}
