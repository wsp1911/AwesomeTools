import type { Candidate } from "../lib/types";
import { kindLabel, kindMeta } from "../lib/colors";
import { formatBytes, truncateMiddle } from "../lib/format";

interface SearchResultsProps {
  results: Candidate[] | null;
  query: string;
  onPick: (candidate: Candidate) => void;
  onClose: () => void;
}

export function SearchResults(props: SearchResultsProps) {
  if (props.results === null) return null;
  return (
    <div className="search-panel">
      <div className="search-panel-head">
        <span>
          找到 {props.results.length} 个匹配 “{props.query}” 的条目
        </span>
        <button className="icon-btn" onClick={props.onClose}>
          关闭
        </button>
      </div>
      <div className="search-list">
        {props.results.map((item) => (
          <button className="search-item" key={item.id} onClick={() => props.onPick(item)}>
            <span className="search-dot" style={{ background: kindMeta(item.kind).color }} />
            <span className="search-main">
              <span className="search-name">{item.name}</span>
              <span className="search-path">{truncateMiddle(item.parentPath, 66)}</span>
            </span>
            <span className="search-kind">{kindLabel(item.kind)}</span>
            <span className="search-size">{formatBytes(item.size)}</span>
          </button>
        ))}
        {props.results.length === 0 ? <div className="search-empty">没有匹配的条目</div> : null}
      </div>
    </div>
  );
}
