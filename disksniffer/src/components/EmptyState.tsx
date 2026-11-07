interface EmptyStateProps {
  drives: string[];
  onScan: (path: string) => void;
  onBrowse: () => void;
}

export function EmptyState(props: EmptyStateProps) {
  return (
    <div className="empty-state">
      <div className="empty-art">
        <span className="art-cell art-a" />
        <span className="art-cell art-b" />
        <span className="art-cell art-c" />
        <span className="art-cell art-d" />
      </div>
      <h1>看清每一块磁盘空间</h1>
      <p className="empty-desc">
        选择一个磁盘或文件夹，DiskSniffer 会并行扫描并把每个文件按体积画成树图。
      </p>

      {props.drives.length > 0 ? (
        <div className="drive-grid">
          {props.drives.map((drive) => (
            <button className="drive-chip" key={drive} onClick={() => props.onScan(drive)}>
              <span className="drive-icon" />
              {drive}
            </button>
          ))}
        </div>
      ) : null}

      <button className="btn btn-primary empty-cta" onClick={props.onBrowse}>
        选择文件夹…
      </button>
      <div className="empty-tips">也可以把文件夹直接拖进窗口</div>
    </div>
  );
}
