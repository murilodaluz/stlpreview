import { useCallback, useEffect, useMemo, useState, type ReactElement } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  errorMessage,
  getSettings,
  listenScanProgress,
  loadLibrary,
  openInOrca,
  openManyInOrca,
  pickFolder,
  pickImage,
  refreshLibrary,
  renameModel,
  revealInFolder,
  saveSettings,
  setCustomPreview,
  setProject,
} from "./api";
import type { ModelActions, ProjectActions } from "./actions";
import {
  ancestorsOf,
  buildGridEntries,
  findFolder,
  itemsUnder,
  type GridEntry,
} from "./grouping";
import { FolderHeader } from "./components/FolderHeader";
import { FolderTree } from "./components/FolderTree";
import { ModelGrid } from "./components/ModelGrid";
import { PreviewModal } from "./components/PreviewModal";
import { SettingsPanel } from "./components/SettingsPanel";
import { Toolbar } from "./components/Toolbar";
import type { AppSettings, FolderNode, Library, ModelItem, ScanProgress } from "./types";

const EMPTY_SETTINGS: AppSettings = { lastRoot: null, orcaPath: null };

export default function App(): ReactElement {
  const [settings, setSettings] = useState<AppSettings>(EMPTY_SETTINGS);
  const [library, setLibrary] = useState<Library | null>(null);
  const [selected, setSelected] = useState<string>("");
  const [includeSubfolders, setIncludeSubfolders] = useState<boolean>(true);
  const [query, setQuery] = useState<string>("");
  const [scanning, setScanning] = useState<boolean>(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState<boolean>(false);
  const [previewPath, setPreviewPath] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set<string>([""]));
  const [ready, setReady] = useState<boolean>(false);

  useEffect((): void => {
    const start = async (): Promise<void> => {
      try {
        const loaded: AppSettings = await getSettings();
        setSettings(loaded);
        if (loaded.lastRoot !== null) {
          const lib: Library = await loadLibrary(loaded.lastRoot);
          setLibrary(lib);
        }
      } catch (caught: unknown) {
        setError(errorMessage(caught));
      } finally {
        setReady(true);
      }
    };
    void start();
  }, []);

  useEffect((): (() => void) => {
    let cancelled: boolean = false;
    let unlisten: UnlistenFn | null = null;
    listenScanProgress((next: ScanProgress): void => setProgress(next)).then(
      (fn: UnlistenFn): void => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      },
      (caught: unknown): void => setError(errorMessage(caught)),
    );
    return (): void => {
      cancelled = true;
      if (unlisten !== null) {
        unlisten();
      }
    };
  }, []);

  const applyLibrary = useCallback((lib: Library): void => {
    setLibrary(lib);
    setSelected((previous: string): string =>
      findFolder(lib.tree, previous) !== null ? previous : "",
    );
  }, []);

  const root: string | null = library?.root ?? settings.lastRoot;

  const handleChoose = useCallback(async (): Promise<void> => {
    try {
      const picked: string | null = await pickFolder();
      if (picked === null) {
        return;
      }
      const next: AppSettings = { ...settings, lastRoot: picked };
      await saveSettings(next);
      setSettings(next);
      const lib: Library = await loadLibrary(picked);
      setLibrary(lib);
      setSelected("");
      setQuery("");
      setExpanded(new Set<string>([""]));
      setPreviewPath(null);
    } catch (caught: unknown) {
      setError(errorMessage(caught));
    }
  }, [settings]);

  const handleRefresh = useCallback(async (): Promise<void> => {
    if (root === null) {
      return;
    }
    setScanning(true);
    setProgress(null);
    try {
      const lib: Library = await refreshLibrary(root);
      applyLibrary(lib);
    } catch (caught: unknown) {
      setError(errorMessage(caught));
    } finally {
      setScanning(false);
      setProgress(null);
    }
  }, [root, applyLibrary]);

  const handleSaveSettings = useCallback(async (next: AppSettings): Promise<void> => {
    try {
      await saveSettings(next);
      setSettings(next);
    } catch (caught: unknown) {
      setError(errorMessage(caught));
    }
  }, []);

  const report = useCallback((caught: unknown): void => setError(errorMessage(caught)), []);

  const mutate = useCallback(
    async (run: (libraryRoot: string) => Promise<Library>): Promise<Library | null> => {
      if (library === null) {
        return null;
      }
      try {
        const next: Library = await run(library.root);
        applyLibrary(next);
        return next;
      } catch (caught: unknown) {
        report(caught);
        try {
          applyLibrary(await loadLibrary(library.root));
        } catch {
          return null;
        }
        return null;
      }
    },
    [library, applyLibrary, report],
  );

  const modelActions: ModelActions = useMemo(
    (): ModelActions => ({
      onPreview: (item: ModelItem): void => setPreviewPath(item.path),
      onOpen: async (item: ModelItem): Promise<void> => {
        try {
          await openInOrca(item.path);
        } catch (caught: unknown) {
          report(caught);
        }
      },
      onReveal: async (item: ModelItem): Promise<void> => {
        try {
          await revealInFolder(item.path);
        } catch (caught: unknown) {
          report(caught);
        }
      },
      onRename: async (item: ModelItem, newName: string): Promise<void> => {
        const knownPaths: ReadonlySet<string> = new Set<string>(
          (library?.items ?? []).map((entry: ModelItem): string => entry.path),
        );
        const next: Library | null = await mutate(
          (libraryRoot: string): Promise<Library> => renameModel(libraryRoot, item.relPath, newName),
        );
        if (next !== null && previewPath === item.path) {
          const renamed: ModelItem | undefined = next.items.find(
            (entry: ModelItem): boolean =>
              !knownPaths.has(entry.path) && entry.folderRelPath === item.folderRelPath,
          );
          setPreviewPath(renamed !== undefined ? renamed.path : null);
        }
      },
      onChoosePreview: async (item: ModelItem): Promise<void> => {
        try {
          const picked: string | null = await pickImage();
          if (picked === null) {
            return;
          }
          await mutate(
            (libraryRoot: string): Promise<Library> =>
              setCustomPreview(libraryRoot, item.relPath, false, picked),
          );
        } catch (caught: unknown) {
          report(caught);
        }
      },
      onResetPreview: async (item: ModelItem): Promise<void> => {
        await mutate(
          (libraryRoot: string): Promise<Library> =>
            setCustomPreview(libraryRoot, item.relPath, false, null),
        );
      },
    }),
    [library, previewPath, mutate, report],
  );

  const projectActions: ProjectActions = useMemo(
    (): ProjectActions => ({
      onNavigate: (relPath: string): void => {
        setSelected(relPath);
        setExpanded((previous: ReadonlySet<string>): ReadonlySet<string> => {
          const next: Set<string> = new Set<string>(previous);
          for (const ancestor of ancestorsOf(relPath)) {
            next.add(ancestor);
          }
          return next;
        });
      },
      onOpenAll: async (node: FolderNode): Promise<void> => {
        if (library === null) {
          return;
        }
        const paths: string[] = itemsUnder(library.items, node.relPath).map(
          (item: ModelItem): string => item.path,
        );
        if (paths.length === 0) {
          setError("This project has no parts to open");
          return;
        }
        try {
          await openManyInOrca(paths);
        } catch (caught: unknown) {
          report(caught);
        }
      },
      onChoosePreview: async (node: FolderNode): Promise<void> => {
        try {
          const picked: string | null = await pickImage();
          if (picked === null) {
            return;
          }
          await mutate(
            (libraryRoot: string): Promise<Library> =>
              setCustomPreview(libraryRoot, node.relPath, true, picked),
          );
        } catch (caught: unknown) {
          report(caught);
        }
      },
      onResetPreview: async (node: FolderNode): Promise<void> => {
        await mutate(
          (libraryRoot: string): Promise<Library> =>
            setCustomPreview(libraryRoot, node.relPath, true, null),
        );
      },
      onSetProject: async (node: FolderNode, isProject: boolean): Promise<void> => {
        await mutate(
          (libraryRoot: string): Promise<Library> =>
            setProject(libraryRoot, node.relPath, isProject),
        );
      },
    }),
    [library, mutate, report],
  );

  const toggleExpanded = useCallback((relPath: string): void => {
    setExpanded((previous: ReadonlySet<string>): ReadonlySet<string> => {
      const next: Set<string> = new Set<string>(previous);
      if (next.has(relPath)) {
        next.delete(relPath);
      } else {
        next.add(relPath);
      }
      return next;
    });
  }, []);

  const handleClosePreview = useCallback((): void => setPreviewPath(null), []);

  const entries: GridEntry[] = useMemo((): GridEntry[] => {
    if (library === null) {
      return [];
    }
    return buildGridEntries({
      tree: library.tree,
      items: library.items,
      selected,
      includeSubfolders,
      query,
    });
  }, [library, selected, includeSubfolders, query]);

  const selectedNode: FolderNode | null = useMemo(
    (): FolderNode | null => (library === null ? null : findFolder(library.tree, selected)),
    [library, selected],
  );

  const previewItem: ModelItem | null = useMemo(
    (): ModelItem | null =>
      previewPath === null || library === null
        ? null
        : (library.items.find((item: ModelItem): boolean => item.path === previewPath) ?? null),
    [library, previewPath],
  );

  const renderMain = (): ReactElement => {
    if (!ready) {
      return <div className="empty" />;
    }
    if (library === null) {
      return (
        <div className="empty">
          <h2>Pick a folder to get started</h2>
          <p>Choose a folder containing .stl and .3mf files.</p>
          <button type="button" className="btn btn-primary" onClick={handleChoose}>
            Choose folder
          </button>
        </div>
      );
    }
    if (library.items.length === 0) {
      return (
        <div className="empty">
          <h2>No previews yet</h2>
          <p>Press Refresh to scan this folder for .stl and .3mf files.</p>
        </div>
      );
    }
    return (
      <>
        {selectedNode !== null ? (
          <FolderHeader
            node={selectedNode}
            includeSubfolders={includeSubfolders}
            actions={projectActions}
          />
        ) : null}
        {entries.length === 0 ? (
          <div className="empty">
            <h2>{query.trim() !== "" ? "Nothing matches" : "Nothing in this folder"}</h2>
          </div>
        ) : (
          <ModelGrid
            entries={entries}
            modelActions={modelActions}
            projectActions={projectActions}
          />
        )}
      </>
    );
  };

  return (
    <div className="app">
      <Toolbar
        root={root}
        scanning={scanning}
        progress={progress}
        query={query}
        settingsOpen={settingsOpen}
        onChoose={handleChoose}
        onRefresh={handleRefresh}
        onQueryChange={setQuery}
        onToggleSettings={(): void => setSettingsOpen((open: boolean): boolean => !open)}
      />
      <div className="body">
        {library !== null ? (
          <FolderTree
            key={library.root}
            tree={library.tree}
            selected={selected}
            includeSubfolders={includeSubfolders}
            expanded={expanded}
            onToggle={toggleExpanded}
            onSelect={setSelected}
            onIncludeChange={setIncludeSubfolders}
          />
        ) : null}
        <main className="main">
          {error !== null ? (
            <div className="toast" role="alert">
              <span>{error}</span>
              <button
                type="button"
                className="toast-close"
                aria-label="Dismiss"
                onClick={(): void => setError(null)}
              >
                &times;
              </button>
            </div>
          ) : null}
          {settingsOpen ? (
            <SettingsPanel
              settings={settings}
              root={root}
              onSave={handleSaveSettings}
              onError={setError}
            />
          ) : null}
          <div className="content">{renderMain()}</div>
        </main>
      </div>
      {previewItem !== null ? (
        <PreviewModal item={previewItem} actions={modelActions} onClose={handleClosePreview} />
      ) : null}
    </div>
  );
}
