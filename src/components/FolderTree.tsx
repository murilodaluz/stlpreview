import type { ChangeEvent, MouseEvent, ReactElement } from "react";
import type { FolderNode } from "../types";

type FolderTreeProps = {
  tree: FolderNode;
  selected: string;
  includeSubfolders: boolean;
  expanded: ReadonlySet<string>;
  onToggle: (relPath: string) => void;
  onSelect: (relPath: string) => void;
  onIncludeChange: (include: boolean) => void;
};

type TreeNodeProps = {
  node: FolderNode;
  depth: number;
  selected: string;
  expanded: ReadonlySet<string>;
  onToggle: (relPath: string) => void;
  onSelect: (relPath: string) => void;
};

function TreeNode(props: TreeNodeProps): ReactElement {
  const hasChildren: boolean = props.node.children.length > 0;
  const isOpen: boolean = props.expanded.has(props.node.relPath);
  const isSelected: boolean = props.selected === props.node.relPath;

  return (
    <li>
      <div
        className={isSelected ? "tree-row selected" : "tree-row"}
        style={{ paddingLeft: `${10 + props.depth * 14}px` }}
        onClick={(): void => props.onSelect(props.node.relPath)}
      >
        <button
          type="button"
          className={hasChildren ? "tree-caret" : "tree-caret hidden"}
          aria-label={isOpen ? "Collapse" : "Expand"}
          onClick={(event: MouseEvent<HTMLButtonElement>): void => {
            event.stopPropagation();
            props.onToggle(props.node.relPath);
          }}
        >
          {isOpen ? "▾" : "▸"}
        </button>
        <span className="tree-name" title={props.node.name}>
          {props.node.name}
        </span>
        {props.node.isProject ? (
          <span className="tree-project" title="Project" aria-label="Project">
            &#9670;
          </span>
        ) : null}
        <span className="tree-count">{props.node.totalCount}</span>
      </div>
      {hasChildren && isOpen ? (
        <ul className="tree-list">
          {props.node.children.map(
            (child: FolderNode): ReactElement => (
              <TreeNode
                key={child.relPath}
                node={child}
                depth={props.depth + 1}
                selected={props.selected}
                expanded={props.expanded}
                onToggle={props.onToggle}
                onSelect={props.onSelect}
              />
            ),
          )}
        </ul>
      ) : null}
    </li>
  );
}

export function FolderTree(props: FolderTreeProps): ReactElement {
  return (
    <aside className="sidebar">
      <label className="include-toggle">
        <input
          type="checkbox"
          checked={props.includeSubfolders}
          onChange={(event: ChangeEvent<HTMLInputElement>): void =>
            props.onIncludeChange(event.target.checked)
          }
        />
        <span>Include subfolders</span>
      </label>
      <ul className="tree-list tree-root">
        <TreeNode
          node={props.tree}
          depth={0}
          selected={props.selected}
          expanded={props.expanded}
          onToggle={props.onToggle}
          onSelect={props.onSelect}
        />
      </ul>
    </aside>
  );
}
