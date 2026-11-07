import { useCallback, useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";

import { api, copyText } from "./lib/api";
import type {
  BatchEntry,
  Candidate,
  ChildInfo,
  ChildrenPage,
  NavEntry,
  NodeSelection,
  ProgressPayload,
  Summary,
} from "./lib/types";
import { formatCount } from "./lib/format";

import { Toolbar } from "./components/Toolbar";
import { Breadcrumb } from "./components/Breadcrumb";
import { Treemap } from "./components/Treemap";
import { ListView } from "./components/ListView";
import { Sidebar } from "./components/Sidebar";
import { EmptyState } from "./components/EmptyState";
import { ScanOverlay } from "./components/ScanOverlay";
import { ContextMenu } from "./components/ContextMenu";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { SearchResults } from "./components/SearchResults";

type ViewMode = "treemap" | "list";

interface MenuState {
  x: number;
  y: number;
  node: NodeSelection;
}

interface ToastState {
  text: string;
  kind: "info" | "error";
}

export default function App() {
  const [summary, setSummary] = useState<Summary | null>(null);
  const [stack, setStack] = useState<NavEntry[]>([]);
  const [page, setPage] = useState<ChildrenPage | null>(null);
  const [nested, setNested] = useState<Record<number, ChildInfo[]>>({});
  const [topFiles, setTopFiles] = useState<Candidate[]>([]);
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ProgressPayload | null>(null);
  const [pathInput, setPathInput] = useState("");
  const [view, setView] = useState<ViewMode>("treemap");
  const [selection, setSelection] = useState<NodeSelection | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Candidate[] | null>(null);
  const [contextMenu, setContextMenu] = useState<MenuState | null>(null);
  const [pendingDelete, setPendingDelete] = useState<NodeSelection | null>(null);
  const [toast, setToast] = useState<ToastState | null>(null);
  const [drives, setDrives] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [refreshToken, setRefreshToken] = useState(0);

  const toastTimer = useRef<number | null>(null);
  const current = stack.length > 0 ? stack[stack.length - 1] : null;
  // Never render a page that belongs to another directory while the next one loads.
  const visiblePage = page && current && page.parent === current.id ? page : null;
  const focusSize = visiblePage
    ? visiblePage.children.reduce((sum, child) => sum + child.size, 0) + visiblePage.omittedSize
    : 0;

  const showToast = useCallback((text: string, kind: "info" | "error" = "info") => {
    setToast({ text, kind });
    if (toastTimer.current !== null) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 4200);
  }, []);

  const reloadTopFiles = useCallback(() => {
    api
      .topFiles(24)
      .then((files) => setTopFiles(files))
      .catch(() => setTopFiles([]));
  }, []);

  const restoreSummary = useCallback(
    (data: Summary) => {
      setSummary(data);
      setPathInput(data.root);
      setStack([{ id: data.rootId, name: data.root, path: data.root }]);
      reloadTopFiles();
    },
    [reloadTopFiles]
  );

  const handleScanDone = useCallback(
    (payload: Summary) => {
      setScanning(false);
      setProgress(null);
      setSummary(payload);
      setSelection(null);
      setStack([{ id: payload.rootId, name: payload.root, path: payload.root }]);
      setPathInput(payload.root);
      setQuery("");
      setResults(null);
      reloadTopFiles();
    },
    [reloadTopFiles]
  );

  useEffect(() => {
    let disposed = false;
    const handles: Array<Promise<UnlistenFn>> = [];

    handles.push(
      listen<ProgressPayload>("scan:progress", (event) => {
        if (!disposed) setProgress(event.payload);
      })
    );
    handles.push(
      listen<Summary>("scan:done", (event) => {
        if (!disposed) handleScanDone(event.payload);
      })
    );
    handles.push(
      listen<string>("scan:failed", (event) => {
        if (disposed) return;
        setScanning(false);
        setProgress(null);
        showToast(event.payload, "error");
      })
    );
    handles.push(
      listen("scan:cancelled", () => {
        if (disposed) return;
        setScanning(false);
        setProgress(null);
        showToast("已停止扫描", "info");
        api
          .getSummary()
          .then((data) => {
            if (data && !disposed) restoreSummary(data);
          })
          .catch(() => undefined);
      })
    );

    return () => {
      disposed = true;
      for (const handle of handles) {
        handle.then((fn) => fn()).catch(() => undefined);
      }
    };
  }, [handleScanDone, restoreSummary, showToast]);

  useEffect(() => {
    api
      .listDrives()
      .then((value) => setDrives(value))
      .catch(() => setDrives([]));
    api
      .getSummary()
      .then((data) => {
        if (data) restoreSummary(data);
      })
      .catch(() => undefined);
  }, [restoreSummary]);

  useEffect(() => {
    if (!current) {
      setPage(null);
      setNested({});
      return;
    }
    let alive = true;
    setLoading(true);
    setPage(null);
    setNested({});
    api
      .getChildren(current.id, 500)
      .then(async (result) => {
        if (!alive) return;
        setPage(result);
        setLoading(false);
        const dirs = result.children
          .filter((child) => child.isDir && child.size > 0)
          .slice(0, 36)
          .map((child) => child.id);
        if (dirs.length === 0) {
          setNested({});
          return;
        }
        const batch = await api
          .getChildrenBatch(dirs, 64)
          .catch(() => [] as BatchEntry[]);
        if (!alive) return;
        const map: Record<number, ChildInfo[]> = {};
        for (const entry of batch) map[entry.id] = entry.children;
        setNested(map);
      })
      .catch((error) => {
        if (!alive) return;
        setLoading(false);
        showToast(String(error), "error");
      });
    return () => {
      alive = false;
    };
  }, [current ? current.id : null, refreshToken, showToast]);

  const focusNode = useCallback((entry: { id: number; name: string; path: string }) => {
    setStack((previous) => {
      const index = previous.findIndex((item) => item.id === entry.id);
      if (index >= 0) return previous.slice(0, index + 1);
      return previous.concat([{ id: entry.id, name: entry.name, path: entry.path }]);
    });
    setQuery("");
    setResults(null);
  }, []);

  const goUp = useCallback(() => {
    setStack((previous) => (previous.length > 1 ? previous.slice(0, -1) : previous));
  }, []);

  const jumpTo = useCallback((index: number) => {
    setStack((previous) => (index >= 0 && index < previous.length ? previous.slice(0, index + 1) : previous));
  }, []);

  const startScan = useCallback(
    async (target: string) => {
      const path = target.trim();
      if (!path) {
        showToast("请输入要扫描的路径", "error");
        return;
      }
      setPathInput(path);
      setScanning(true);
      setProgress({ files: 0, dirs: 0, bytes: 0, current: path });
      setSummary(null);
      setPage(null);
      setNested({});
      setSelection(null);
      setStack([]);
      setQuery("");
      setResults(null);
      try {
        await api.startScan(path);
      } catch (error) {
        setScanning(false);
        setProgress(null);
        showToast(String(error), "error");
      }
    },
    [showToast]
  );

  const cancelScan = useCallback(async () => {
    try {
      await api.cancelScan();
    } catch (error) {
      showToast(String(error), "error");
    }
  }, [showToast]);

  const browse = useCallback(async () => {
    try {
      const picked = await openFolderDialog({
        directory: true,
        multiple: false,
        title: "选择要扫描的文件夹",
      });
      if (typeof picked === "string") {
        void startScan(picked);
      }
    } catch (error) {
      showToast(String(error), "error");
    }
  }, [startScan, showToast]);

  const reveal = useCallback(
    async (path: string) => {
      try {
        await api.revealPath(path);
      } catch (error) {
        showToast(String(error), "error");
      }
    },
    [showToast]
  );

  const openDirect = useCallback(
    async (path: string) => {
      try {
        await api.openPath(path);
      } catch (error) {
        showToast(String(error), "error");
      }
    },
    [showToast]
  );

  const copyPath = useCallback(
    async (path: string) => {
      try {
        await copyText(path);
        showToast("路径已复制", "info");
      } catch {
        showToast("复制失败", "error");
      }
    },
    [showToast]
  );

  const openCandidate = useCallback(
    async (candidate: Candidate) => {
      try {
        const chain = await api.getChain(candidate.id);
        const nav = chain.map((entry) => ({
          id: entry.id,
          name: entry.name,
          path: entry.path,
        }));
        if (candidate.isDir) {
          setStack(nav);
          setSelection(null);
        } else {
          setStack(nav.slice(0, Math.max(1, nav.length - 1)));
          setSelection({
            id: candidate.id,
            name: candidate.name,
            path: candidate.path,
            size: candidate.size,
            isDir: candidate.isDir,
            kind: candidate.kind,
            childCount: candidate.childCount,
          });
        }
        setQuery("");
        setResults(null);
      } catch (error) {
        showToast(String(error), "error");
      }
    },
    [showToast]
  );

  const requestTrash = useCallback(
    (node: NodeSelection) => {
      if (summary && node.id === summary.rootId) {
        showToast("不能删除扫描的根目录", "error");
        return;
      }
      setPendingDelete(node);
    },
    [summary, showToast]
  );

  const confirmTrash = useCallback(async () => {
    const node = pendingDelete;
    setPendingDelete(null);
    if (!node) return;
    try {
      await api.trashNode(node.id);
      showToast("已移到回收站：" + node.name, "info");
      setSelection(null);
      const data = await api.getSummary().catch(() => null);
      if (data) setSummary(data);
      reloadTopFiles();
      setRefreshToken((token) => token + 1);
    } catch (error) {
      showToast(String(error), "error");
    }
  }, [pendingDelete, reloadTopFiles, showToast]);

  useEffect(() => {
    if (!summary) {
      setResults(null);
      return;
    }
    const trimmed = query.trim();
    if (trimmed.length === 0) {
      setResults(null);
      return;
    }
    const timer = window.setTimeout(() => {
      api
        .search(trimmed, 80)
        .then((hits) => setResults(hits))
        .catch(() => setResults([]));
    }, 220);
    return () => window.clearTimeout(timer);
  }, [query, summary]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing =
        target !== null && (target.tagName === "INPUT" || target.tagName === "TEXTAREA");
      if (event.key === "Escape") {
        if (contextMenu) {
          setContextMenu(null);
          return;
        }
        if (pendingDelete) {
          setPendingDelete(null);
          return;
        }
        if (results) {
          setResults(null);
          setQuery("");
          return;
        }
        if (typing && target) target.blur();
        return;
      }
      if (typing) return;
      if (event.key === "Backspace" || (event.altKey && event.key === "ArrowUp")) {
        event.preventDefault();
        goUp();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [contextMenu, pendingDelete, results, goUp]);

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    import("@tauri-apps/api/webview")
      .then((mod) =>
        mod.getCurrentWebview().onDragDropEvent((event) => {
          const payload = event.payload as { type: string; paths?: string[] };
          if (payload.type === "drop" && payload.paths && payload.paths.length > 0) {
            void startScan(payload.paths[0]);
          }
        })
      )
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [startScan]);

  const openMenu = (event: ReactMouseEvent, node: NodeSelection) => {
    setContextMenu({ x: event.clientX, y: event.clientY, node });
  };

  return (
    <div className="app">
      <Toolbar
        pathInput={pathInput}
        onPathInput={setPathInput}
        onScan={() => void startScan(pathInput)}
        onBrowse={() => void browse()}
        onCancel={() => void cancelScan()}
        scanning={scanning}
        query={query}
        onQuery={setQuery}
        view={view}
        onView={setView}
        hasResult={summary !== null}
      />

      <div className="main">
        <Sidebar
          summary={summary}
          topFiles={topFiles}
          selection={selection}
          scanning={scanning}
          onOpenCandidate={(candidate) => void openCandidate(candidate)}
          onReveal={(path) => void reveal(path)}
          onCopy={(path) => void copyPath(path)}
          onTrash={requestTrash}
          onFocusNode={(node) => focusNode(node)}
        />

        <section className="workspace">
          {summary ? (
            <Breadcrumb
              stack={stack}
              totalSize={focusSize}
              canUp={stack.length > 1}
              onUp={goUp}
              onJump={jumpTo}
            />
          ) : null}

          <div className="canvas">
            {!summary && !scanning ? (
              <EmptyState
                drives={drives}
                onScan={(path) => void startScan(path)}
                onBrowse={() => void browse()}
              />
            ) : null}

            {summary && !scanning && current ? (
              view === "treemap" ? (
                <Treemap
                  children={visiblePage ? visiblePage.children : []}
                  nested={nested}
                  loading={loading}
                  parentPath={current.path}
                  parentId={current.id}
                  totalSize={focusSize}
                  selectedId={selection ? selection.id : null}
                  onFocus={focusNode}
                  onSelect={setSelection}
                  onReveal={(path) => void reveal(path)}
                  onContextMenu={openMenu}
                />
              ) : (
                <ListView
                  page={visiblePage}
                  loading={loading}
                  parentPath={current.path}
                  totalSize={focusSize}
                  selectedId={selection ? selection.id : null}
                  onFocus={focusNode}
                  onSelect={setSelection}
                  onContextMenu={openMenu}
                />
              )
            ) : null}

            {loading && summary ? <div className="loading-bar" /> : null}

            <SearchResults
              results={results}
              query={query}
              onPick={(candidate) => void openCandidate(candidate)}
              onClose={() => {
                setResults(null);
                setQuery("");
              }}
            />

            {scanning ? (
              <ScanOverlay progress={progress} onCancel={() => void cancelScan()} />
            ) : null}
          </div>

          <div className="statusbar">
            <span className="status-left" title={current ? current.path : ""}>
              {current ? current.path : "尚未扫描"}
            </span>
            <span className="status-right">
              {visiblePage
                ? formatCount(visiblePage.totalChildren) +
                  " 个直接子项" +
                  (visiblePage.omitted > 0
                    ? " · " + formatCount(visiblePage.omitted) + " 项已折叠"
                    : "")
                : scanning
                ? "扫描中…"
                : loading
                ? "加载中…"
                : ""}
            </span>
          </div>
        </section>
      </div>

      {contextMenu ? (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          node={contextMenu.node}
          onClose={() => setContextMenu(null)}
          onOpen={(path) => void openDirect(path)}
          onReveal={(path) => void reveal(path)}
          onCopyPath={(path) => void copyPath(path)}
          onFocus={(node) => focusNode(node)}
          onTrash={requestTrash}
        />
      ) : null}

      {pendingDelete ? (
        <ConfirmDialog
          title="移到回收站？"
          message={
            <span>
              即将把 <b>{pendingDelete.name}</b> 移动到系统回收站，之后仍可以从回收站还原。
            </span>
          }
          detail={pendingDelete.path}
          confirmLabel="移到回收站"
          danger
          onConfirm={() => void confirmTrash()}
          onCancel={() => setPendingDelete(null)}
        />
      ) : null}

      {toast ? <div className={"toast " + toast.kind}>{toast.text}</div> : null}
    </div>
  );
}
