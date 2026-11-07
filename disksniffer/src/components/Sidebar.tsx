import type { Candidate, NodeSelection, Summary } from "../lib/types";
import { kindLabel, kindMeta } from "../lib/colors";
import {
  formatBytes,
  formatCount,
  formatDuration,
  formatPercent,
  truncateMiddle,
} from "../lib/format";

interface SidebarProps {
  summary: Summary | null;
  topFiles: Candidate[];
  selection: NodeSelection | null;
  scanning: boolean;
  onOpenCandidate: (candidate: Candidate) => void;
  onReveal: (path: string) => void;
  onCopy: (text: string) => void;
  onTrash: (node: NodeSelection) => void;
  onFocusNode: (node: NodeSelection) => void;
}

export function Sidebar(props: SidebarProps) {
  const summary = props.summary;
  const kinds = summary
    ? summary.kinds
        .filter((kind) => kind.size > 0)
        .slice()
        .sort((a, b) => b.size - a.size)
    : [];

  return (
    <aside className="sidebar">
      {summary ? (
        <>
          <section className="side-section">
            <div className="side-title">概览</div>
            <div
              className="hero-size"
              title="按磁盘实际占用统计：硬链接只计一次，云端占位文件按真实占用计，不含 NTFS 元数据"
            >
              {formatBytes(summary.totalSize)}
            </div>
            <div className="hero-path" title={summary.root}>
              {summary.root}
            </div>
            <div className="stat-grid">
              <div>
                <span>文件</span>
                <strong>{formatCount(summary.totalFiles)}</strong>
              </div>
              <div>
                <span>文件夹</span>
                <strong>{formatCount(summary.totalDirs)}</strong>
              </div>
              <div>
                <span>耗时</span>
                <strong>{formatDuration(summary.elapsedMs)}</strong>
              </div>
              <div>
                <span>无法访问</span>
                <strong>{formatCount(summary.errors)}</strong>
              </div>
            </div>
          </section>

          <section className="side-section">
            <div className="side-title">类型分布</div>
            {kinds.map((kind) => {
              const meta = kindMeta(kind.key);
              return (
                <div className="kind-row" key={kind.key}>
                  <span className="kind-dot" style={{ background: meta.color }} />
                  <span className="kind-label">{kindLabel(kind.key)}</span>
                  <span className="kind-size">{formatBytes(kind.size)}</span>
                  <span className="kind-percent">{formatPercent(kind.size, summary.totalSize)}</span>
                  <div className="kind-bar">
                    <div
                      style={{
                        width: Math.max(2, (kind.size / Math.max(summary.totalSize, 1)) * 100) + "%",
                        background: meta.color,
                      }}
                    />
                  </div>
                </div>
              );
            })}
            {kinds.length === 0 ? <div className="side-empty">没有统计到文件</div> : null}
          </section>

          <section className="side-section">
            <div className="side-title">最大的文件</div>
            <div className="top-files">
              {props.topFiles.map((file) => (
                <button
                  className="top-file"
                  key={file.id}
                  onClick={() => props.onOpenCandidate(file)}
                  title={file.path}
                >
                  <span className="top-file-bar" style={{ background: kindMeta(file.kind).color }} />
                  <span className="top-file-main">
                    <span className="top-file-name">{file.name}</span>
                    <span className="top-file-path">{truncateMiddle(file.parentPath, 42)}</span>
                  </span>
                  <span className="top-file-size">{formatBytes(file.size)}</span>
                </button>
              ))}
              {props.topFiles.length === 0 ? <div className="side-empty">暂无数据</div> : null}
            </div>
          </section>
        </>
      ) : (
        <section className="side-section">
          <div className="side-title">概览</div>
          <div className="side-empty">还没有扫描结果</div>
        </section>
      )}

      {props.selection ? (
        <section className="side-section selection-card">
          <div className="side-title">已选择</div>
          <div className="selection-name" title={props.selection.name}>
            {props.selection.name}
          </div>
          <div className="selection-path" title={props.selection.path}>
            {props.selection.path}
          </div>
          <div className="selection-facts">
            <span>{formatBytes(props.selection.size)}</span>
            <span className="dot">·</span>
            <span>{kindLabel(props.selection.kind)}</span>
            {props.selection.isDir ? (
              <>
                <span className="dot">·</span>
                <span>{formatCount(props.selection.childCount)} 项</span>
              </>
            ) : null}
          </div>
          <div className="selection-actions">
            {props.selection.isDir ? (
              <button className="chip" onClick={() => props.onFocusNode(props.selection!)}>
                进入目录
              </button>
            ) : null}
            <button className="chip" onClick={() => props.onReveal(props.selection!.path)}>
              定位文件
            </button>
            <button className="chip" onClick={() => props.onCopy(props.selection!.path)}>
              复制路径
            </button>
            <button className="chip danger" onClick={() => props.onTrash(props.selection!)}>
              移到回收站
            </button>
          </div>
        </section>
      ) : null}
    </aside>
  );
}
