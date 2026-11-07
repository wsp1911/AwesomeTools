import { useMemo, useState, type MouseEvent as ReactMouseEvent } from "react";
import type { ChildrenPage, NodeSelection } from "../lib/types";
import { kindLabel, kindMeta } from "../lib/colors";
import { formatBytes, formatCount, formatPercent } from "../lib/format";

interface ListViewProps {
  page: ChildrenPage | null;
  loading: boolean;
  parentPath: string;
  totalSize: number;
  selectedId: number | null;
  onFocus: (entry: { id: number; name: string; path: string }) => void;
  onSelect: (selection: NodeSelection) => void;
  onContextMenu: (event: ReactMouseEvent, selection: NodeSelection) => void;
}

type SortKey = "size" | "name";

export function ListView(props: ListViewProps) {
  const [sortKey, setSortKey] = useState<SortKey>("size");
  const [descending, setDescending] = useState(true);

  const rows = useMemo(() => {
    const children = props.page ? props.page.children.slice() : [];
    const separator = props.parentPath.indexOf("\\") >= 0 ? "\\" : "/";
    const withPath = children.map((child) => ({
      ...child,
      path:
        props.parentPath.charAt(props.parentPath.length - 1) === separator
          ? props.parentPath + child.name
          : props.parentPath + separator + child.name,
    }));
    withPath.sort((a, b) => {
      let result: number;
      if (sortKey === "name") {
        result = a.name.localeCompare(b.name, "zh-Hans-CN");
      } else {
        result = a.size - b.size;
      }
      return descending ? -result : result;
    });
    return withPath;
  }, [props.page, props.parentPath, sortKey, descending]);

  const toggle = (key: SortKey) => {
    if (key === sortKey) {
      setDescending(!descending);
    } else {
      setSortKey(key);
      setDescending(key === "size");
    }
  };

  const total = props.totalSize || 1;

  return (
    <div className="list-view">
      <table className="list-table">
        <thead>
          <tr>
            <th className="col-name" onClick={() => toggle("name")}>
              名称 {sortKey === "name" ? (descending ? "▾" : "▴") : ""}
            </th>
            <th className="col-size" onClick={() => toggle("size")}>
              大小 {sortKey === "size" ? (descending ? "▾" : "▴") : ""}
            </th>
            <th className="col-share">占比</th>
            <th className="col-kind">类型</th>
            <th className="col-count">子项</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const meta = kindMeta(row.kind);
            const selection: NodeSelection = {
              id: row.id,
              name: row.name,
              path: row.path,
              size: row.size,
              isDir: row.isDir,
              kind: row.kind,
              childCount: row.childCount,
            };
            return (
              <tr
                key={row.id}
                className={props.selectedId === row.id ? "is-selected" : ""}
                onClick={() => {
                  props.onSelect(selection);
                  if (row.isDir) {
                    props.onFocus({ id: row.id, name: row.name, path: row.path });
                  }
                }}
                onContextMenu={(event) => {
                  event.preventDefault();
                  props.onContextMenu(event, selection);
                }}
              >
                <td className="col-name">
                  <span className="row-icon" style={{ background: meta.color }}>
                    {row.isDir ? "▣" : "▤"}
                  </span>
                  <span className="row-name">{row.name}</span>
                </td>
                <td className="col-size">{formatBytes(row.size)}</td>
                <td className="col-share">
                  <div className="share-bar">
                    <div
                      className="share-fill"
                      style={{
                        width: Math.min(100, (row.size / total) * 100) + "%",
                        background: meta.color,
                      }}
                    />
                  </div>
                  <span className="share-text">{formatPercent(row.size, total)}</span>
                </td>
                <td className="col-kind">{kindLabel(row.kind)}</td>
                <td className="col-count">{row.isDir ? formatCount(row.childCount) : "-"}</td>
              </tr>
            );
          })}
          {props.page && props.page.omitted > 0 ? (
            <tr className="row-omitted">
              <td className="col-name">其他 {formatCount(props.page.omitted)} 项（未显示）</td>
              <td className="col-size">{formatBytes(props.page.omittedSize)}</td>
              <td className="col-share" colSpan={3} />
            </tr>
          ) : null}
        </tbody>
      </table>
      {rows.length === 0 && !props.loading ? (
        <div className="list-empty">此文件夹没有可显示的内容</div>
      ) : null}
    </div>
  );
}
