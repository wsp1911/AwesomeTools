import type { NavEntry } from "../lib/types";
import { formatBytes } from "../lib/format";

interface BreadcrumbProps {
  stack: NavEntry[];
  totalSize: number;
  canUp: boolean;
  onUp: () => void;
  onJump: (index: number) => void;
}

export function Breadcrumb(props: BreadcrumbProps) {
  return (
    <div className="breadcrumb-bar">
      <button
        className="icon-btn back-btn"
        onClick={props.onUp}
        disabled={!props.canUp}
        title="返回上级 (Backspace)"
      >
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
          <path
            d="M6.5 3L2.5 7l4 4M2.8 7h6.4c2.3 0 4.2 1.7 4.2 4v1"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>

      <div className="crumbs">
        {props.stack.map((entry, index) => {
          const isLast = index === props.stack.length - 1;
          return (
            <span className="crumb-group" key={entry.id + "-" + index}>
              {index > 0 ? <span className="crumb-sep">›</span> : null}
              <button
                className={"crumb" + (isLast ? " current" : "")}
                onClick={() => props.onJump(index)}
                title={entry.path}
              >
                {entry.name}
              </button>
            </span>
          );
        })}
      </div>

      <div className="crumb-meta">{formatBytes(props.totalSize)}</div>
    </div>
  );
}
