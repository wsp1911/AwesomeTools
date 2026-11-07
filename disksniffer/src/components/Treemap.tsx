import { useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import type { ChildInfo, NodeSelection } from "../lib/types";
import { layoutTreemap, pickCellAt, type LaidOutRect } from "../lib/treemap";
import { kindLabel, kindMeta, paintRect } from "../lib/colors";
import { formatBytes, formatCount, formatPercent } from "../lib/format";

interface TreemapProps {
  children: ChildInfo[];
  nested: Record<number, ChildInfo[]>;
  parentPath: string;
  parentId: number;
  totalSize: number;
  selectedId: number | null;
  loading: boolean;
  onFocus: (entry: { id: number; name: string; path: string }) => void;
  onSelect: (selection: NodeSelection) => void;
  onReveal: (path: string) => void;
  onContextMenu: (event: ReactMouseEvent, selection: NodeSelection) => void;
}

export function Treemap(props: TreemapProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const [pointer, setPointer] = useState({ x: 0, y: 0 });

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const update = () => {
      setSize({ width: element.clientWidth, height: element.clientHeight });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const rects = useMemo(
    () =>
      layoutTreemap({
        children: props.children,
        nested: props.nested,
        parentPath: props.parentPath,
        parentId: props.parentId,
        width: size.width,
        height: size.height,
      }),
    [props.children, props.nested, props.parentPath, props.parentId, size.width, size.height]
  );

  // Hit testing is geometric: the deepest cell that contains the pointer wins.
  // Deriving it from the DOM target instead would go wrong whenever a cell is
  // painted above its children, which is exactly what hovering a container did.
  const hitTest = (event: ReactMouseEvent): { rect: LaidOutRect; index: number } | null => {
    const element = containerRef.current;
    if (!element || rects.length === 0) return null;
    const box = element.getBoundingClientRect();
    const rect = pickCellAt(rects, event.clientX - box.left, event.clientY - box.top);
    if (!rect) return null;
    return { rect, index: rects.indexOf(rect) };
  };

  const pick = (event: ReactMouseEvent): LaidOutRect | null => hitTest(event)?.rect ?? null;

  const toSelection = (rect: LaidOutRect): NodeSelection => ({
    id: rect.id,
    name: rect.name,
    path: rect.path,
    size: rect.size,
    isDir: rect.isDir,
    kind: rect.kind,
    childCount: rect.childCount,
  });

  const handleClick = (event: ReactMouseEvent) => {
    const rect = pick(event);
    if (!rect || rect.aggregated) return;
    props.onSelect(toSelection(rect));
    if (rect.isDir) {
      props.onFocus({ id: rect.id, name: rect.name, path: rect.path });
    }
  };

  const handleDoubleClick = (event: ReactMouseEvent) => {
    const rect = pick(event);
    if (!rect || rect.aggregated) return;
    if (!rect.isDir) props.onReveal(rect.path);
  };

  const handleContextMenu = (event: ReactMouseEvent) => {
    const rect = pick(event);
    if (!rect || rect.aggregated) return;
    event.preventDefault();
    props.onContextMenu(event, toSelection(rect));
  };

  const hovered = hoverIndex !== null && hoverIndex < rects.length ? rects[hoverIndex] : null;

  return (
    <div
      className="treemap"
      ref={containerRef}
      onMouseMove={(event) => {
        setPointer({ x: event.clientX, y: event.clientY });
        const hit = hitTest(event);
        setHoverIndex(hit ? hit.index : null);
      }}
      onMouseLeave={() => setHoverIndex(null)}
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
      onContextMenu={handleContextMenu}
    >
      {rects.map((rect, index) => {
        const paint = paintRect(rect.kind, rect.isDir, rect.depth);
        // SpaceSniffer convention: containers carry a one line header,
        // leaves show their information centered in the cell.
        const isLeaf = !rect.subdivided;
        const showCaptionSize = rect.w >= (rect.depth === 0 ? 124 : 100);
        const showLeafName = rect.w >= 26 && rect.h >= 12;
        const showLeafSize = rect.w >= 52 && rect.h >= 26;

        const classes = ["tm-rect"];
        if (hoverIndex === index) classes.push("is-hover");
        if (props.selectedId !== null && props.selectedId === rect.id) classes.push("is-selected");
        if (rect.isDir) classes.push("is-dir");
        return (
          <div
            key={String(rect.id) + "-" + String(rect.depth) + "-" + String(index)}
            className={classes.join(" ")}
            data-rect={index}
            style={{
              left: rect.x.toFixed(2) + "px",
              top: rect.y.toFixed(2) + "px",
              width: Math.max(0, rect.w).toFixed(2) + "px",
              height: Math.max(0, rect.h).toFixed(2) + "px",
              background: paint.background,
              backgroundImage: paint.backgroundImage,
              boxShadow: paint.boxShadow,
            }}
          >
            {!isLeaf ? (
              <div
                className="tm-caption"
                style={{ height: Math.round(rect.captionHeight) + "px" }}
              >
                <span className="tm-caption-name">{rect.name}</span>
                {showCaptionSize ? (
                  <span className="tm-caption-size">{"- " + formatBytes(rect.size)}</span>
                ) : null}
              </div>
            ) : showLeafName ? (
              <div className="tm-leaf">
                <span className="tm-leaf-name">{rect.name}</span>
                {showLeafSize ? (
                  <span className="tm-leaf-size">{formatBytes(rect.size)}</span>
                ) : null}
              </div>
            ) : null}
          </div>
        );
      })}

      {rects.length === 0 && !props.loading ? (
        <div className="treemap-empty">此文件夹没有可显示的内容（可能为空或没有访问权限）</div>
      ) : null}

      {hovered ? (
        <div
          className="tm-tooltip"
          style={{
            left: Math.min(pointer.x + 16, window.innerWidth - 300) + "px",
            top: Math.min(pointer.y + 18, window.innerHeight - 130) + "px",
          }}
        >
          <div className="tm-tooltip-title" style={{ color: kindMeta(hovered.kind).color }}>
            {hovered.name}
          </div>
          <div className="tm-tooltip-row">
            <span>{formatBytes(hovered.size)}</span>
            <span className="dot">·</span>
            <span>{formatPercent(hovered.size, props.totalSize)}</span>
            <span className="dot">·</span>
            <span>{kindLabel(hovered.kind)}</span>
          </div>
          {hovered.isDir ? (
            <div className="tm-tooltip-row subtle">{formatCount(hovered.childCount)} 个直接子项</div>
          ) : null}
          {hovered.path ? <div className="tm-tooltip-path">{hovered.path}</div> : null}
          <div className="tm-tooltip-hint">
            {hovered.isDir ? "单击进入 · 右键更多操作" : "单击选中 · 双击定位文件"}
          </div>
        </div>
      ) : null}
    </div>
  );
}
