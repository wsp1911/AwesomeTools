import { layoutTreemap, pickCellAt, squarify } from "../src/lib/treemap.ts";

function approx(a: number, b: number, tolerance: number) {
  return Math.abs(a - b) <= tolerance;
}

let failures = 0;
function check(name: string, condition: boolean, extra?: string) {
  if (condition) {
    console.log("PASS  " + name);
  } else {
    failures += 1;
    console.log("FAIL  " + name + (extra ? "  -> " + extra : ""));
  }
}

const source = [];
for (let i = 0; i < 80; i += 1) {
  source.push({
    id: i + 1,
    name: "entry-" + i,
    size: Math.round(500000 / (i + 1)) + 1,
    isDir: i % 2 === 0,
    childCount: 5,
    kind: i % 4 === 0 ? "video" : "image",
  });
}
source.sort((a, b) => b.size - a.size);

const nested: Record<number, any[]> = {};
for (const item of source) {
  if (!item.isDir) continue;
  const kids = [];
  for (let k = 0; k < 12; k += 1) {
    kids.push({
      id: 10000 + item.id * 10 + k,
      name: "sub-" + k,
      size: Math.max(1, Math.round(item.size / (k + 2))),
      isDir: k % 3 === 0,
      childCount: 3,
      kind: "code",
    });
  }
  nested[item.id] = kids;
}

const W = 1000;
const H = 600;
const rects = layoutTreemap({
  children: source,
  nested,
  parentPath: "C:\\root",
  parentId: 0,
  width: W,
  height: H,
});

const level0 = rects.filter((rect) => rect.depth === 0);

check("level 0 has all entries", level0.length === source.length, String(level0.length));

let areaSum = 0;
let outOfBounds = 0;
let badAspect = 0;
for (const rect of level0) {
  areaSum += rect.w * rect.h;
  if (rect.x < -0.01 || rect.y < -0.01 || rect.x + rect.w > W + 0.5 || rect.y + rect.h > H + 0.5) {
    outOfBounds += 1;
  }
  const ratio = Math.max(rect.w / Math.max(rect.h, 0.001), rect.h / Math.max(rect.w, 0.001));
  if (ratio > 14) badAspect += 1;
}

check("level 0 covers the box within padding", areaSum >= W * H * 0.9, String(areaSum));
check("level 0 stays inside the box", outOfBounds === 0, String(outOfBounds));
check("level 0 keeps readable aspect ratios", badAspect === 0, badAspect + " cells over 14:1");

const deeper = rects.filter((rect) => rect.depth > 0);
check("nested levels are produced", deeper.length > 0, String(deeper.length));

let outsideParent = 0;
for (const rect of deeper) {
  const parent = rects.find(
    (candidate) =>
      candidate.isDir &&
      candidate.depth < rect.depth &&
      rect.x >= candidate.x - 0.01 &&
      rect.y >= candidate.y - 0.01 &&
      rect.x + rect.w <= candidate.x + candidate.w + 0.5 &&
      rect.y + rect.h <= candidate.y + candidate.h + 0.5
  );
  if (!parent) outsideParent += 1;
}
check("nested cells stay inside a parent cell", outsideParent === 0, String(outsideParent));

const paths = rects.every((rect) => rect.id < 0 || rect.path.startsWith("C:\\root\\"));
check("paths are built from the parent path", paths);

const byId = new Map(rects.map((rect) => [rect.id, rect]));
let captionClash = 0;
let orphanNested = 0;
for (const rect of rects) {
  if (rect.depth === 0) continue;
  const parent = byId.get(rect.parentId);
  if (!parent) {
    orphanNested += 1;
    continue;
  }
  if (rect.y < parent.y + parent.captionHeight - 0.51) captionClash += 1;
}
check("nested cells always start below the parent caption", captionClash === 0, String(captionClash));
check("every nested cell has a laid out parent", orphanNested === 0, String(orphanNested));
check(
  "captions never exceed their cell",
  rects.every((rect) => rect.captionHeight > 0 && rect.captionHeight <= rect.h + 0.01)
);
check(
  "subdivided cells reserve a caption strip",
  rects.every((rect) => !rect.subdivided || rect.captionHeight >= 14)
);

// --- hit testing -----------------------------------------------------------
// The highlight must follow the deepest cell under the pointer. A container is
// laid out flat next to its children, so any hit test that depends on paint
// order would keep reporting the container while the pointer sits on a child.
const subdivided = rects.filter((rect) => rect.subdivided);
let samples = 0;
let childShadowed = 0;
let captionStolen = 0;
for (const parent of subdivided) {
  const kids = rects.filter((rect) => rect.depth === parent.depth + 1 && rect.parentId === parent.id);
  if (kids.length === 0) continue;
  samples += 1;
  const kid = kids[kids.length - 1];
  if (pickCellAt(rects, kid.x + kid.w / 2, kid.y + kid.h / 2) !== kid) childShadowed += 1;
  const capY = parent.y + parent.captionHeight / 2;
  if (pickCellAt(rects, parent.x + Math.min(10, parent.w / 2), capY) !== parent) captionStolen += 1;
}
check("subdivided containers are exercised", samples > 0, String(samples));
check("pointer inside a child resolves to the child", childShadowed === 0, String(childShadowed));
check("pointer on the container caption resolves to the container", captionStolen === 0, String(captionStolen));
check("pointer outside every cell resolves to nothing", pickCellAt(rects, -20, -20) === null);

// Painting order must never decide the hit: build a copy where every container
// is painted after its children (the situation that used to freeze the hover).
const paintedOnTop = [...rects].sort((a, b) => b.depth - a.depth);
let orderSensitive = 0;
for (const parent of subdivided) {
  const kids = rects.filter((rect) => rect.depth === parent.depth + 1 && rect.parentId === parent.id);
  if (kids.length === 0) continue;
  const kid = kids[kids.length - 1];
  if (pickCellAt(paintedOnTop, kid.x + kid.w / 2, kid.y + kid.h / 2) !== kid) orderSensitive += 1;
}
check("hit testing ignores paint order", orderSensitive === 0, String(orderSensitive));

// A sealed pair: two siblings separated by the usual hairline gap, so the seam
// between them belongs to nobody. It must resolve to the closer sibling rather
// than the parent, which is what used to make the highlight flash.
const pair = [
  { id: 10, depth: 0, x: 0, y: 0, w: 100, h: 100, captionHeight: 15, subdivided: true, parentId: 0 },
  { id: 11, depth: 1, x: 1, y: 17, w: 47, h: 82, captionHeight: 15, subdivided: false, parentId: 10 },
  { id: 12, depth: 1, x: 50, y: 17, w: 49, h: 82, captionHeight: 15, subdivided: false, parentId: 10 },
] as any[];
check("seam snaps to the left sibling", pickCellAt(pair, 48.5, 60)?.id === 11);
check("seam snaps to the right sibling", pickCellAt(pair, 49.5, 60)?.id === 12);
check("seam never snaps back to the parent", pickCellAt(pair, 49, 60)?.id !== 10);
check("parent caption still resolves to the parent", pickCellAt(pair, 20, 7)?.id === 10);
check("gap under the caption snaps to a child", pickCellAt(pair, 20, 16)?.id === 11);

// Nothing inside a container's body may resolve back to the container.
let bodyFallsBack = 0;
let bodySamples = 0;
for (const parent of subdivided) {
  const top = parent.y + parent.captionHeight;
  const bottom = parent.y + parent.h;
  for (let i = 1; i < 12; i += 1) {
    for (let j = 1; j < 12; j += 1) {
      const x = parent.x + (parent.w * i) / 12;
      const y = top + ((bottom - top) * j) / 12;
      bodySamples += 1;
      const hit = pickCellAt(rects, x, y);
      if (!hit || hit.id === parent.id) bodyFallsBack += 1;
    }
  }
}
check("container bodies always resolve to a descendant", bodyFallsBack === 0, bodyFallsBack + "/" + bodySamples);

const raw = squarify(
  source.map((item) => ({ id: item.id, value: item.size })),
  { x: 0, y: 0, w: W, h: H }
);
const rawArea = raw.reduce((sum, cell) => sum + cell.w * cell.h, 0);
check("squarify fills the box exactly", approx(rawArea, W * H, W * H * 0.0001), String(rawArea));

let overlap = 0;
for (let i = 0; i < raw.length; i += 1) {
  for (let j = i + 1; j < raw.length; j += 1) {
    const a = raw[i];
    const b = raw[j];
    const dx = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
    const dy = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
    if (dx > 0.5 && dy > 0.5) overlap += 1;
  }
}
check("squarify cells do not overlap", overlap === 0, String(overlap));

const tiny = squarify([{ id: 1, value: 1 }, { id: 2, value: 0 }], { x: 0, y: 0, w: 100, h: 100 });
check("zero sized values are skipped", tiny.length === 1);
check("zero area boxes are skipped", squarify([{ id: 1, value: 5 }], { x: 0, y: 0, w: 0, h: 100 }).length === 0);

console.log(failures === 0 ? "ALL CHECKS PASSED" : failures + " CHECK(S) FAILED");
if (failures > 0) process.exit(1);
