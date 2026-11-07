import type { ChildInfo } from "./types";

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PlacedBox extends Box {
  id: number;
  value: number;
}

function contains(rect: LaidOutRect, x: number, y: number): boolean {
  if (rect.w <= 0 || rect.h <= 0) return false;
  return x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h;
}

/** Squared distance from the point to the cell, 0 when the point is inside it. */
function distanceTo(rect: LaidOutRect, x: number, y: number): number {
  const dx = Math.max(rect.x - x, 0, x - (rect.x + rect.w));
  const dy = Math.max(rect.y - y, 0, y - (rect.y + rect.h));
  return dx * dx + dy * dy;
}

/**
 * Deepest cell containing the point, or null when the point misses everything.
 * Nested cells only ever sit inside exactly one ancestor, so "deepest" is
 * unambiguous and never depends on paint order.
 *
 * Cells are inset by a hairline gap, so the seam between two siblings falls on
 * their parent. Seams resolve to the closest child instead of the parent: the
 * highlight would otherwise flick back to the parent for a frame while the
 * pointer travels between two neighbouring cells. The parent keeps its caption
 * strip, which is how it is highlighted and clicked.
 */
export function pickCellAt(rects: LaidOutRect[], x: number, y: number): LaidOutRect | null {
  let deepest: LaidOutRect | null = null;
  for (const rect of rects) {
    if (!contains(rect, x, y)) continue;
    if (!deepest || rect.depth > deepest.depth) deepest = rect;
  }
  if (!deepest) return null;
  if (!deepest.subdivided) return deepest;
  if (y < deepest.y + deepest.captionHeight) return deepest;

  let nearest: LaidOutRect | null = null;
  let nearestDistance = Number.POSITIVE_INFINITY;
  for (const rect of rects) {
    if (rect.depth !== deepest.depth + 1 || rect.parentId !== deepest.id) continue;
    if (rect.w <= 0 || rect.h <= 0) continue;
    const distance = distanceTo(rect, x, y);
    if (distance < nearestDistance) {
      nearestDistance = distance;
      nearest = rect;
    }
  }
  return nearest ?? deepest;
}

export interface LaidOutRect extends Box {
  id: number;
  name: string;
  size: number;
  isDir: boolean;
  childCount: number;
  kind: string;
  path: string;
  parentId: number;
  depth: number;
  aggregated: boolean;
  subdivided: boolean;
  captionHeight: number;
}

function worstRatio(row: { area: number }[], side: number): number {
  let sum = 0;
  let max = 0;
  let min = Number.POSITIVE_INFINITY;
  for (const item of row) {
    sum += item.area;
    if (item.area > max) max = item.area;
    if (item.area < min) min = item.area;
  }
  if (sum <= 0 || side <= 0) return Number.POSITIVE_INFINITY;
  return Math.max(
    (side * side * max) / (sum * sum),
    (sum * sum) / (side * side * Math.max(min, 1e-9))
  );
}

/**
 * Squarified treemap layout (Bruls, Huizing & van Wijk).
 * Items are laid out in descending size order to keep aspect ratios readable.
 */
export function squarify(items: { id: number; value: number }[], box: Box): PlacedBox[] {
  const result: PlacedBox[] = [];
  const total = items.reduce((sum, item) => sum + item.value, 0);
  if (total <= 0 || box.w <= 0 || box.h <= 0) return result;

  const area = box.w * box.h;
  const scaled = items
    .filter((item) => item.value > 0)
    .map((item) => ({
      id: item.id,
      value: item.value,
      area: (item.value / total) * area,
    }))
    .sort((a, b) => b.area - a.area);

  let x = box.x;
  let y = box.y;
  let w = box.w;
  let h = box.h;
  let index = 0;

  while (index < scaled.length) {
    const side = Math.min(w, h);
    const row = [scaled[index]];
    let cursor = index + 1;
    let best = worstRatio(row, side);

    while (cursor < scaled.length) {
      const candidate = row.concat(scaled[cursor]);
      const ratio = worstRatio(candidate, side);
      if (ratio <= best) {
        row.push(scaled[cursor]);
        best = ratio;
        cursor += 1;
      } else {
        break;
      }
    }

    const rowArea = row.reduce((sum, item) => sum + item.area, 0);

    if (w >= h) {
      const columnWidth = Math.min(rowArea / Math.max(h, 1e-6), w);
      let offset = y;
      for (const item of row) {
        const itemHeight = rowArea > 0 ? (item.area / rowArea) * h : 0;
        result.push({ id: item.id, value: item.value, x, y: offset, w: columnWidth, h: itemHeight });
        offset += itemHeight;
      }
      x += columnWidth;
      w -= columnWidth;
    } else {
      const rowHeight = Math.min(rowArea / Math.max(w, 1e-6), h);
      let offset = x;
      for (const item of row) {
        const itemWidth = rowArea > 0 ? (item.area / rowArea) * w : 0;
        result.push({ id: item.id, value: item.value, x: offset, y, w: itemWidth, h: rowHeight });
        offset += itemWidth;
      }
      y += rowHeight;
      h -= rowHeight;
    }

    index = cursor;
  }

  return result;
}

export interface LayoutOptions {
  children: ChildInfo[];
  nested: Record<number, ChildInfo[]>;
  parentPath: string;
  parentId: number;
  width: number;
  height: number;
  maxDepth?: number;
  maxItems?: number;
}

export function layoutTreemap(options: LayoutOptions): LaidOutRect[] {
  const { children, nested, parentPath, parentId, width, height } = options;
  const maxDepth = options.maxDepth ?? 2;
  const maxItems = options.maxItems ?? 220;
  const out: LaidOutRect[] = [];
  if (width < 10 || height < 10 || !children.length) return out;

  const separator = parentPath.indexOf("\\") >= 0 ? "\\" : "/";
  const join = (base: string, name: string): string => {
    if (!base) return name;
    if (base.charAt(base.length - 1) === separator) return base + name;
    return base + separator + name;
  };

  const place = (
    kids: ChildInfo[],
    aggregated: { size: number; count: number } | null,
    box: Box,
    depth: number,
    basePath: string,
    ownerId: number
  ) => {
    const values: { id: number; value: number }[] = [];
    const lookup = new Map<number, ChildInfo>();
    for (const kid of kids) {
      if (kid.size <= 0) continue;
      lookup.set(kid.id, kid);
      values.push({ id: kid.id, value: kid.size });
    }
    if (aggregated && aggregated.size > 0) {
      values.push({ id: -1, value: aggregated.size });
    }
    if (!values.length) return;

    const placed = squarify(values, box);
    const captionHeight = depth === 0 ? 17 : 15;

    for (const cell of placed) {
      const pad = depth === 0 ? 1.5 : 1;
      const x = cell.x + pad;
      const y = cell.y + pad;
      const w = cell.w - pad * 2;
      const h = cell.h - pad * 2;
      if (w < 1.5 || h < 1.5) continue;

      const source = lookup.get(cell.id);
      const isDir = source ? source.isDir : false;
      // Everything nested is laid out strictly below this node's own caption
      // strip, so a label can never collide with the labels of its children.
      const childKids =
        isDir && cell.id >= 0 && depth < maxDepth ? nested[cell.id] : undefined;
      const subdivide = !!childKids && childKids.length > 0 && w > 56 && h > 48;
      const caption = Math.min(captionHeight, h);

      const rect: LaidOutRect = source
        ? {
            id: source.id,
            name: source.name,
            size: source.size,
            isDir: source.isDir,
            childCount: source.childCount,
            kind: source.kind,
            path: join(basePath, source.name),
            parentId: ownerId,
            depth,
            aggregated: false,
            subdivided: subdivide,
            captionHeight: caption,
            x,
            y,
            w,
            h,
          }
        : {
            id: -1,
            name: "其他 " + String(aggregated ? aggregated.count : 0) + " 项",
            size: aggregated ? aggregated.size : cell.value,
            isDir: false,
            childCount: aggregated ? aggregated.count : 0,
            kind: "other",
            path: "",
            parentId: ownerId,
            depth,
            aggregated: true,
            subdivided: false,
            captionHeight: caption,
            x,
            y,
            w,
            h,
          };
      out.push(rect);

      if (subdivide && childKids) {
        const inner: Box = {
          x: x + 1,
          y: y + caption + 1,
          w: w - 2,
          h: h - caption - 1,
        };
        if (inner.w > 10 && inner.h > 10) {
          place(childKids, null, inner, depth + 1, rect.path, rect.id);
        }
      }
    }
  };

  const live = children.filter((child) => child.size > 0);
  let visible = live;
  let omittedSize = 0;
  let omittedCount = 0;
  if (live.length > maxItems) {
    visible = live.slice(0, maxItems);
    for (let i = maxItems; i < live.length; i += 1) {
      omittedSize += live[i].size;
      omittedCount += 1;
    }
  }

  place(
    visible,
    omittedCount > 0 ? { size: omittedSize, count: omittedCount } : null,
    { x: 0, y: 0, w: width, h: height },
    0,
    parentPath,
    parentId
  );

  return out;
}
