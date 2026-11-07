use disksniffer_core::tree::ChildInfo;
use std::collections::HashMap;

pub const MAX_DEPTH: u32 = 3;
pub const MAX_ITEMS: usize = 220;

/// Sentinel id for the synthetic "other items" cell.
pub const AGGREGATED_ID: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug)]
pub struct LaidOutRect {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub child_count: u32,
    pub kind: String,
    pub path: String,
    pub parent_id: u32,
    pub depth: u32,
    pub aggregated: bool,
    pub subdivided: bool,
    pub caption_height: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl LaidOutRect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && x >= self.x
            && y >= self.y
            && x < self.x + self.w
            && y < self.y + self.h
    }

    fn distance_sq(&self, x: f32, y: f32) -> f32 {
        let dx = (self.x - x).max(0.0).max(x - (self.x + self.w));
        let dy = (self.y - y).max(0.0).max(y - (self.y + self.h));
        dx * dx + dy * dy
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub id: i64,
    pub value: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

fn worst_ratio(areas: &[f32], side: f32) -> f32 {
    let mut sum = 0.0f32;
    let mut max: f32 = 0.0;
    let mut min = f32::INFINITY;
    for area in areas {
        sum += *area;
        max = max.max(*area);
        min = min.min(*area);
    }
    if sum <= 0.0 || side <= 0.0 {
        return f32::INFINITY;
    }
    ((side * side * max) / (sum * sum)).max((sum * sum) / (side * side * min.max(1e-9)))
}

/// Squarified treemap layout (Bruls, Huizing and van Wijk). Items are placed in
/// descending size order so the aspect ratios stay readable.
pub fn squarify(items: &[(i64, u64)], area: Area) -> Vec<Cell> {
    let mut out = Vec::new();
    if area.w <= 0.0 || area.h <= 0.0 {
        return out;
    }
    let total: u64 = items.iter().map(|(_, value)| *value).sum();
    if total == 0 {
        return out;
    }

    let full = area.w * area.h;
    let mut scaled: Vec<(i64, u64, f32)> = items
        .iter()
        .filter(|(_, value)| *value > 0)
        .map(|(id, value)| (*id, *value, (*value as f32 / total as f32) * full))
        .collect();
    scaled.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let (mut x, mut y, mut w, mut h) = (area.x, area.y, area.w, area.h);
    let mut index = 0usize;

    while index < scaled.len() {
        let side = w.min(h);
        let mut areas = vec![scaled[index].2];
        let mut best = worst_ratio(&areas, side);
        let mut cursor = index + 1;
        while cursor < scaled.len() {
            areas.push(scaled[cursor].2);
            let ratio = worst_ratio(&areas, side);
            if ratio <= best {
                best = ratio;
                cursor += 1;
            } else {
                areas.pop();
                break;
            }
        }

        let row = &scaled[index..cursor];
        let row_area: f32 = row.iter().map(|item| item.2).sum();

        if w >= h {
            let column_width = (row_area / h.max(1e-6)).min(w);
            let mut offset = y;
            for item in row {
                let item_height = if row_area > 0.0 {
                    item.2 / row_area * h
                } else {
                    0.0
                };
                out.push(Cell {
                    id: item.0,
                    value: item.1,
                    x,
                    y: offset,
                    w: column_width,
                    h: item_height,
                });
                offset += item_height;
            }
            x += column_width;
            w -= column_width;
        } else {
            let row_height = (row_area / w.max(1e-6)).min(h);
            let mut offset = x;
            for item in row {
                let item_width = if row_area > 0.0 {
                    item.2 / row_area * w
                } else {
                    0.0
                };
                out.push(Cell {
                    id: item.0,
                    value: item.1,
                    x: offset,
                    y,
                    w: item_width,
                    h: row_height,
                });
                offset += item_width;
            }
            y += row_height;
            h -= row_height;
        }

        index = cursor;
    }

    out
}

fn join(base: &str, name: &str) -> String {
    if base.is_empty() {
        return name.to_string();
    }
    if base.ends_with(std::path::MAIN_SEPARATOR) {
        format!("{}{}", base, name)
    } else {
        format!("{}{}{}", base, std::path::MAIN_SEPARATOR, name)
    }
}

#[allow(clippy::too_many_arguments)]
fn place(
    nested: &HashMap<u32, Vec<ChildInfo>>,
    out: &mut Vec<LaidOutRect>,
    kids: &[ChildInfo],
    aggregated: Option<(u64, u32)>,
    area: Area,
    depth: u32,
    base_path: &str,
    owner_id: u32,
) {
    let mut values: Vec<(i64, u64)> = kids.iter().map(|kid| (kid.id as i64, kid.size)).collect();
    if let Some((size, _)) = aggregated {
        if size > 0 {
            values.push((-1, size));
        }
    }
    if values.is_empty() {
        return;
    }

    let cells = squarify(&values, area);
    let caption_height: f32 = if depth == 0 { 17.0 } else { 15.0 };

    for cell in cells {
        let pad = if depth == 0 { 1.5 } else { 1.0 };
        let x = cell.x + pad;
        let y = cell.y + pad;
        let w = cell.w - pad * 2.0;
        let h = cell.h - pad * 2.0;
        if w < 1.5 || h < 1.5 {
            continue;
        }

        let source = if cell.id >= 0 {
            kids.iter().find(|kid| kid.id as i64 == cell.id)
        } else {
            None
        };
        let is_dir = source.map(|kid| kid.is_dir).unwrap_or(false);
        // Everything nested is laid out strictly below this node's caption
        // strip, so labels can never collide with the labels of its children.
        let kids_of = if is_dir && depth < MAX_DEPTH {
            nested.get(&(cell.id as u32))
        } else {
            None
        };
        let subdivide =
            kids_of.map(|list| !list.is_empty()).unwrap_or(false) && w > 56.0 && h > 48.0;
        let caption = caption_height.min(h);

        let rect = match source {
            Some(kid) => LaidOutRect {
                id: kid.id,
                name: kid.name.clone(),
                size: kid.size,
                is_dir: kid.is_dir,
                child_count: kid.child_count,
                kind: kid.kind.clone(),
                path: join(base_path, &kid.name),
                parent_id: owner_id,
                depth,
                aggregated: false,
                subdivided: subdivide,
                caption_height: caption,
                x,
                y,
                w,
                h,
            },
            None => {
                let (size, count) = aggregated.unwrap_or((cell.value, 0));
                LaidOutRect {
                    id: AGGREGATED_ID,
                    name: format!("其他 {} 项", count),
                    size,
                    is_dir: false,
                    child_count: count,
                    kind: "other".to_string(),
                    path: String::new(),
                    parent_id: owner_id,
                    depth,
                    aggregated: true,
                    subdivided: false,
                    caption_height: caption,
                    x,
                    y,
                    w,
                    h,
                }
            }
        };

        let child_path = rect.path.clone();
        let child_id = rect.id;
        out.push(rect);

        if subdivide {
            if let Some(grand) = kids_of {
                let inner = Area {
                    x: x + 1.0,
                    y: y + caption + 1.0,
                    w: w - 2.0,
                    h: h - caption - 1.0,
                };
                if inner.w > 10.0 && inner.h > 10.0 {
                    place(
                        nested,
                        out,
                        grand,
                        None,
                        inner,
                        depth + 1,
                        &child_path,
                        child_id,
                    );
                }
            }
        }
    }
}

pub fn layout_treemap(
    children: &[ChildInfo],
    nested: &HashMap<u32, Vec<ChildInfo>>,
    parent_path: &str,
    parent_id: u32,
    width: f32,
    height: f32,
) -> Vec<LaidOutRect> {
    let mut out = Vec::new();
    let live: Vec<&ChildInfo> = children.iter().filter(|kid| kid.size > 0).collect();

    let (visible, aggregated) = if live.len() > MAX_ITEMS {
        let mut omitted_size = 0u64;
        let mut omitted_count = 0u32;
        for kid in &live[MAX_ITEMS..] {
            omitted_size = omitted_size.saturating_add(kid.size);
            omitted_count += 1;
        }
        (&live[..MAX_ITEMS], Some((omitted_size, omitted_count)))
    } else {
        (&live[..], None)
    };

    let owned: Vec<ChildInfo> = visible.iter().map(|kid| (*kid).clone()).collect();
    place(
        nested,
        &mut out,
        &owned,
        aggregated,
        Area {
            x: 0.0,
            y: 0.0,
            w: width,
            h: height,
        },
        0,
        parent_path,
        parent_id,
    );
    out
}

/// Deepest cell containing the point, or the index of it. Nested cells only
/// ever sit inside exactly one ancestor, so "deepest" is unambiguous and never
/// depends on paint order.
///
/// Cells are inset by a hairline gap, so the seam between two siblings falls on
/// their parent. Seams resolve to the closest child instead of the parent: the
/// highlight would otherwise flick back to the parent while the pointer travels
/// between two neighbouring cells. The parent keeps its caption strip.
pub fn pick_cell_at(rects: &[LaidOutRect], x: f32, y: f32) -> Option<usize> {
    let mut deepest: Option<usize> = None;
    for (index, rect) in rects.iter().enumerate() {
        if !rect.contains(x, y) {
            continue;
        }
        match deepest {
            Some(best) if rects[best].depth >= rect.depth => {}
            _ => deepest = Some(index),
        }
    }

    let deepest = deepest?;
    let parent = &rects[deepest];
    if !parent.subdivided {
        return Some(deepest);
    }
    if y < parent.y + parent.caption_height {
        return Some(deepest);
    }

    let mut nearest: Option<usize> = None;
    let mut nearest_distance = f32::INFINITY;
    for (index, rect) in rects.iter().enumerate() {
        if rect.depth != parent.depth + 1 || rect.parent_id != parent.id {
            continue;
        }
        if rect.w <= 0.0 || rect.h <= 0.0 {
            continue;
        }
        let distance = rect.distance_sq(x, y);
        if distance < nearest_distance {
            nearest_distance = distance;
            nearest = Some(index);
        }
    }
    nearest.or(Some(deepest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kid(id: u32, name: String, size: u64, is_dir: bool, kind: &str) -> ChildInfo {
        ChildInfo {
            id,
            name,
            size,
            is_dir,
            child_count: 5,
            kind: kind.to_string(),
        }
    }

    fn fixture() -> (Vec<ChildInfo>, HashMap<u32, Vec<ChildInfo>>) {
        let mut top = Vec::new();
        for i in 0..80u32 {
            let size = (500_000u64 / (i as u64 + 1)) + 1;
            top.push(kid(
                i + 1,
                format!("entry-{}", i),
                size,
                i % 2 == 0,
                if i % 4 == 0 { "video" } else { "image" },
            ));
        }
        top.sort_by(|a, b| b.size.cmp(&a.size));

        let mut nested = HashMap::new();
        for item in &top {
            if !item.is_dir {
                continue;
            }
            let mut kids = Vec::new();
            for k in 0..12u32 {
                let size = (item.size / (k as u64 + 2)).max(1);
                kids.push(kid(
                    10_000 + item.id * 10 + k,
                    format!("sub-{}", k),
                    size,
                    k % 3 == 0,
                    "code",
                ));
            }
            nested.insert(item.id, kids);
        }
        (top, nested)
    }

    fn laid_out() -> Vec<LaidOutRect> {
        let (top, nested) = fixture();
        layout_treemap(&top, &nested, "C:\\root", 0, 1000.0, 600.0)
    }

    fn plain(
        id: u32,
        depth: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        caption_height: f32,
        subdivided: bool,
        parent_id: u32,
    ) -> LaidOutRect {
        LaidOutRect {
            id,
            name: format!("node-{}", id),
            size: 1,
            is_dir: true,
            child_count: 0,
            kind: "code".to_string(),
            path: String::new(),
            parent_id,
            depth,
            aggregated: false,
            subdivided,
            caption_height,
            x,
            y,
            w,
            h,
        }
    }

    #[test]
    fn level_zero_holds_every_entry() {
        let rects = laid_out();
        assert_eq!(rects.iter().filter(|rect| rect.depth == 0).count(), 80);
    }

    #[test]
    fn level_zero_fills_the_box_and_stays_inside() {
        let rects = laid_out();
        let (width, height) = (1000.0f32, 600.0f32);
        let mut area = 0.0f32;
        for rect in rects.iter().filter(|rect| rect.depth == 0) {
            area += rect.w * rect.h;
            assert!(rect.x >= -0.01 && rect.y >= -0.01);
            assert!(rect.x + rect.w <= width + 0.5);
            assert!(rect.y + rect.h <= height + 0.5);
            let ratio = (rect.w / rect.h.max(0.001)).max(rect.h / rect.w.max(0.001));
            assert!(ratio <= 14.0, "{} has aspect ratio {}", rect.name, ratio);
        }
        assert!(area >= width * height * 0.9, "covered area {}", area);
    }

    #[test]
    fn nested_cells_stay_inside_a_parent() {
        let rects = laid_out();
        let deeper: Vec<&LaidOutRect> = rects.iter().filter(|rect| rect.depth > 0).collect();
        assert!(!deeper.is_empty());

        let mut orphans = 0;
        for rect in &deeper {
            let inside = rects.iter().any(|candidate| {
                candidate.is_dir
                    && candidate.depth < rect.depth
                    && rect.x >= candidate.x - 0.01
                    && rect.y >= candidate.y - 0.01
                    && rect.x + rect.w <= candidate.x + candidate.w + 0.5
                    && rect.y + rect.h <= candidate.y + candidate.h + 0.5
            });
            if !inside {
                orphans += 1;
            }
        }
        assert_eq!(orphans, 0);
    }

    #[test]
    fn nested_cells_never_cover_the_parent_caption() {
        let rects = laid_out();
        let mut clashes = 0;
        let mut missing = 0;
        for rect in rects.iter().filter(|rect| rect.depth > 0) {
            match rects
                .iter()
                .find(|candidate| candidate.id == rect.parent_id)
            {
                Some(parent) => {
                    if rect.y < parent.y + parent.caption_height - 0.51 {
                        clashes += 1;
                    }
                }
                None => missing += 1,
            }
        }
        assert_eq!(clashes, 0);
        assert_eq!(missing, 0);
    }

    #[test]
    fn captions_fit_and_subdivided_cells_reserve_one() {
        let rects = laid_out();
        for rect in &rects {
            assert!(rect.caption_height > 0.0);
            assert!(rect.caption_height <= rect.h + 0.01);
            if rect.subdivided {
                assert!(rect.caption_height >= 14.0);
            }
        }
    }

    #[test]
    fn paths_are_built_from_the_parent_path() {
        let rects = laid_out();
        for rect in &rects {
            if rect.aggregated {
                continue;
            }
            assert!(rect.path.starts_with("C:\\root\\"), "{}", rect.path);
        }
    }

    #[test]
    fn hit_testing_follows_the_deepest_child() {
        let rects = laid_out();
        let parents: Vec<&LaidOutRect> = rects.iter().filter(|rect| rect.subdivided).collect();
        assert!(!parents.is_empty());

        for parent in parents {
            let kids: Vec<&LaidOutRect> = rects
                .iter()
                .filter(|rect| rect.depth == parent.depth + 1 && rect.parent_id == parent.id)
                .collect();
            let Some(kid) = kids.last() else { continue };
            let hit = pick_cell_at(&rects, kid.x + kid.w / 2.0, kid.y + kid.h / 2.0);
            assert_eq!(hit.map(|index| rects[index].id), Some(kid.id));

            let caption_y = parent.y + parent.caption_height / 2.0;
            let hit = pick_cell_at(&rects, parent.x + parent.w.min(10.0), caption_y);
            assert_eq!(hit.map(|index| rects[index].id), Some(parent.id));

            let top = parent.y + parent.caption_height;
            for i in 1..12 {
                for j in 1..12 {
                    let x = parent.x + parent.w * (i as f32) / 12.0;
                    let y = top + (parent.y + parent.h - top) * (j as f32) / 12.0;
                    let hit = pick_cell_at(&rects, x, y);
                    assert!(hit.is_some(), "no hit at {},{}", x, y);
                    assert_ne!(
                        rects[hit.unwrap()].id,
                        parent.id,
                        "parent body leaked at {},{}",
                        x,
                        y
                    );
                }
            }
        }
    }

    #[test]
    fn hit_testing_ignores_paint_order() {
        let rects = laid_out();
        let mut reversed = rects.clone();
        reversed.sort_by(|a, b| b.depth.cmp(&a.depth));
        for parent in rects.iter().filter(|rect| rect.subdivided) {
            let kids: Vec<&LaidOutRect> = rects
                .iter()
                .filter(|rect| rect.depth == parent.depth + 1 && rect.parent_id == parent.id)
                .collect();
            let Some(kid) = kids.last() else { continue };
            let hit = pick_cell_at(&reversed, kid.x + kid.w / 2.0, kid.y + kid.h / 2.0);
            assert_eq!(hit.map(|index| reversed[index].id), Some(kid.id));
        }
    }

    #[test]
    fn pointer_outside_every_cell_hits_nothing() {
        let rects = laid_out();
        assert!(pick_cell_at(&rects, -20.0, -20.0).is_none());
    }

    #[test]
    fn seams_snap_to_the_nearer_sibling() {
        let pair = vec![
            plain(10, 0, 0.0, 0.0, 100.0, 100.0, 15.0, true, 0),
            plain(11, 1, 1.0, 17.0, 47.0, 82.0, 15.0, false, 10),
            plain(12, 1, 50.0, 17.0, 49.0, 82.0, 15.0, false, 10),
        ];
        let at = |x: f32, y: f32| pick_cell_at(&pair, x, y).map(|index| pair[index].id);

        assert_eq!(at(48.5, 60.0), Some(11));
        assert_eq!(at(49.5, 60.0), Some(12));
        assert_eq!(at(49.0, 60.0), Some(11));
        assert_eq!(at(20.0, 7.0), Some(10));
        assert_eq!(at(20.0, 16.0), Some(11));
    }

    #[test]
    fn squarify_fills_the_box_without_overlap() {
        let (top, _) = fixture();
        let items: Vec<(i64, u64)> = top.iter().map(|item| (item.id as i64, item.size)).collect();
        let cells = squarify(
            &items,
            Area {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 600.0,
            },
        );
        let area: f32 = cells.iter().map(|cell| cell.w * cell.h).sum();
        assert!((area - 600_000.0).abs() <= 60.0, "area {}", area);

        let mut overlaps = 0;
        for i in 0..cells.len() {
            for j in (i + 1)..cells.len() {
                let a = &cells[i];
                let b = &cells[j];
                let dx = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let dy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                if dx > 0.5 && dy > 0.5 {
                    overlaps += 1;
                }
            }
        }
        assert_eq!(overlaps, 0);
    }

    #[test]
    fn squarify_skips_degenerate_input() {
        let cells = squarify(
            &[(1, 1), (2, 0)],
            Area {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
        );
        assert_eq!(cells.len(), 1);
        let empty = squarify(
            &[(1, 5)],
            Area {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 100.0,
            },
        );
        assert!(empty.is_empty());
    }
}
