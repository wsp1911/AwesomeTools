interface ToolbarProps {
  pathInput: string;
  onPathInput: (value: string) => void;
  onScan: () => void;
  onBrowse: () => void;
  onCancel: () => void;
  scanning: boolean;
  query: string;
  onQuery: (value: string) => void;
  view: "treemap" | "list";
  onView: (view: "treemap" | "list") => void;
  hasResult: boolean;
}

export function Toolbar(props: ToolbarProps) {
  return (
    <header className="toolbar">
      <div className="brand">
        <span className="brand-mark">
          <span className="brand-cell brand-cell-a" />
          <span className="brand-cell brand-cell-b" />
          <span className="brand-cell brand-cell-c" />
          <span className="brand-cell brand-cell-d" />
        </span>
        <span className="brand-text">DiskSniffer</span>
      </div>

      <div className="path-field">
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
          <path
            d="M1.5 4.2c0-.7.6-1.2 1.2-1.2h3l1.2 1.4h5.4c.7 0 1.2.6 1.2 1.2v6c0 .7-.5 1.2-1.2 1.2H2.7c-.6 0-1.2-.5-1.2-1.2V4.2z"
            fill="currentColor"
            opacity="0.75"
          />
        </svg>
        <input
          value={props.pathInput}
          onChange={(event) => props.onPathInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") props.onScan();
          }}
          placeholder="输入要扫描的磁盘或文件夹路径，例如 C:\ 或 D:\Projects"
          spellCheck={false}
        />
        <button className="icon-btn" onClick={props.onBrowse} title="浏览文件夹">
          浏览
        </button>
      </div>

      {props.scanning ? (
        <button className="btn btn-stop" onClick={props.onCancel}>
          停止扫描
        </button>
      ) : (
        <button className="btn btn-primary" onClick={props.onScan}>
          开始扫描
        </button>
      )}

      <div className="toolbar-right">
        <div className={"search-field" + (props.hasResult ? "" : " is-disabled")}>
          <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
            <circle cx="7" cy="7" r="4.4" fill="none" stroke="currentColor" strokeWidth="1.4" />
            <path d="M10.4 10.4L14 14" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          </svg>
          <input
            value={props.query}
            onChange={(event) => props.onQuery(event.target.value)}
            placeholder="搜索文件或文件夹"
            spellCheck={false}
            disabled={!props.hasResult}
          />
        </div>
        <div className="view-switch">
          <button
            className={props.view === "treemap" ? "is-active" : ""}
            onClick={() => props.onView("treemap")}
            title="树图视图"
          >
            树图
          </button>
          <button
            className={props.view === "list" ? "is-active" : ""}
            onClick={() => props.onView("list")}
            title="列表视图"
          >
            列表
          </button>
        </div>
      </div>
    </header>
  );
}
