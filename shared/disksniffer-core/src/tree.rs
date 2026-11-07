use crate::scan::{kind_key, RawNode};
use memmap2::Mmap;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub const NULL: u32 = u32::MAX;
const FLAG_DIR: u8 = 1 << 0;

/// Packed node record. Every query the UI makes is answered from this record
/// plus the shared name arena, which lets the whole scan result live in a
/// memory-mapped temp file instead of the heap.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NodeRecord {
    name_off: u32,
    name_len: u32,
    parent: u32,
    child_count: u32,
    first_child: u32,
    next_sibling: u32,
    size: u64,
    kind: u8,
    dominant: u8,
    flags: u8,
    reserved: u8,
}

const _: () = assert!(std::mem::size_of::<NodeRecord>() == 40);

impl NodeRecord {
    #[inline]
    fn is_dir(&self) -> bool {
        self.flags & FLAG_DIR != 0
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChildInfo {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub child_count: u32,
    pub kind: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChildrenPage {
    pub parent: u32,
    pub children: Vec<ChildInfo>,
    pub omitted: u32,
    pub omitted_size: u64,
    pub total_children: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BatchEntry {
    pub id: u32,
    pub children: Vec<ChildInfo>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub parent: u32,
    pub parent_path: String,
    pub size: u64,
    pub is_dir: bool,
    pub kind: String,
    pub child_count: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetail {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub parent: u32,
    pub size: u64,
    pub is_dir: bool,
    pub kind: String,
    pub child_count: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PathEntry {
    pub id: u32,
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KindStat {
    pub key: String,
    pub size: u64,
    pub count: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub root: String,
    pub root_id: u32,
    pub total_size: u64,
    pub total_files: u64,
    pub total_dirs: u64,
    pub elapsed_ms: u64,
    pub errors: u64,
    pub kinds: Vec<KindStat>,
}

/// What changed after the scan finished. Moving a subtree to the recycle bin is
/// the only mutation, it is rare, and the index itself stays immutable, so the
/// edit is tracked here instead of in the mapped data.
#[derive(Default)]
struct Overlay {
    removed: Vec<(u32, u32)>,
    shaved: HashMap<u32, u64>,
}

impl Overlay {
    /// Ids are handed out depth first, so a subtree always occupies a
    /// contiguous id range and the check is one interval test.
    fn is_removed(&self, id: u32) -> bool {
        self.removed
            .iter()
            .any(|&(start, end)| id >= start && id < end)
    }

    fn size_of(&self, id: u32, base: u64) -> u64 {
        if self.is_removed(id) {
            return 0;
        }
        base.saturating_sub(*self.shaved.get(&id).unwrap_or(&0))
    }
}

/// Removes the backing files once the mappings above it are gone. Declared last
/// on purpose: fields drop in declaration order and Windows refuses to delete a
/// file that is still mapped.
struct Cleanup(Vec<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

enum Data {
    Heap {
        nodes: Vec<NodeRecord>,
        names: Vec<u8>,
    },
    Mapped {
        nodes: Mmap,
        names: Mmap,
        count: usize,
        // Held purely to delete the files once the mappings are gone.
        _cleanup: Cleanup,
    },
}

impl Data {
    fn nodes(&self) -> &[NodeRecord] {
        match self {
            Data::Heap { nodes, .. } => nodes,
            Data::Mapped { nodes, count, .. } => {
                // Written by this process from a &[NodeRecord] of exactly this
                // layout, so the mapping is a plain array of records.
                unsafe { std::slice::from_raw_parts(nodes.as_ptr() as *const NodeRecord, *count) }
            }
        }
    }

    fn names(&self) -> &[u8] {
        match self {
            Data::Heap { names, .. } => names,
            Data::Mapped { names, .. } => names,
        }
    }

    #[cfg(test)]
    fn is_mapped(&self) -> bool {
        matches!(self, Data::Mapped { .. })
    }
}

fn as_bytes(slice: &[NodeRecord]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice))
    }
}

fn write_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.flush()
}

fn spill_files(nodes: &[NodeRecord], names: &[u8]) -> io::Result<Data> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let base = std::env::temp_dir().join(format!(
        "disksniffer-{}-{}",
        std::process::id(),
        stamp
    ));
    let node_path = base.with_extension("nodes");
    let name_path = base.with_extension("names");
    let cleanup = Cleanup(vec![node_path.clone(), name_path.clone()]);
    write_bytes(&node_path, as_bytes(nodes))?;
    write_bytes(&name_path, names)?;
    let node_map = unsafe { Mmap::map(&File::open(&node_path)?)? };
    let name_map = unsafe { Mmap::map(&File::open(&name_path)?)? };
    Ok(Data::Mapped {
        nodes: node_map,
        names: name_map,
        count: nodes.len(),
        _cleanup: cleanup,
    })
}

/// Moves the finished index out of the heap. A failed spill keeps everything in
/// memory rather than losing the scan.
fn spill(nodes: Vec<NodeRecord>, names: Vec<u8>) -> Data {
    match spill_files(&nodes, &names) {
        Ok(data) => data,
        Err(_) => Data::Heap { nodes, names },
    }
}

struct Builder {
    nodes: Vec<NodeRecord>,
    names: Vec<u8>,
}

fn flatten(raw: RawNode, parent: u32, builder: &mut Builder) -> u32 {
    let RawNode {
        name,
        size,
        is_dir,
        kind,
        dominant,
        children,
    } = raw;

    let index = builder.nodes.len() as u32;
    let name_off = builder.names.len() as u32;
    builder.names.extend_from_slice(name.as_bytes());
    let name_len = builder.names.len() as u32 - name_off;
    let child_count = children.len() as u32;
    builder.nodes.push(NodeRecord {
        name_off,
        name_len,
        parent,
        child_count,
        first_child: NULL,
        next_sibling: NULL,
        size,
        kind,
        dominant,
        flags: if is_dir { FLAG_DIR } else { 0 },
        reserved: 0,
    });
    drop(name);

    let mut previous = NULL;
    for child in children {
        let child_index = flatten(child, index, builder);
        if previous == NULL {
            builder.nodes[index as usize].first_child = child_index;
        } else {
            builder.nodes[previous as usize].next_sibling = child_index;
        }
        previous = child_index;
    }
    index
}

fn count_nodes(raw: &RawNode) -> usize {
    let mut total = 1;
    for child in &raw.children {
        total += count_nodes(child);
    }
    total
}

pub struct ScanTree {
    data: Data,
    pub root: u32,
    pub root_path: PathBuf,
    pub errors: u64,
    pub elapsed_ms: u64,
    overlay: RwLock<Overlay>,
}

impl ScanTree {
    pub fn build(raw: RawNode, root_path: PathBuf, errors: u64, elapsed_ms: u64) -> ScanTree {
        let expected = count_nodes(&raw);
        let mut builder = Builder {
            nodes: Vec::with_capacity(expected),
            names: Vec::with_capacity(expected * 12),
        };
        let root = flatten(raw, NULL, &mut builder);
        let data = spill(builder.nodes, builder.names);
        ScanTree {
            data,
            root,
            root_path,
            errors,
            elapsed_ms,
            overlay: RwLock::new(Overlay::default()),
        }
    }

    #[inline]
    fn nodes(&self) -> &[NodeRecord] {
        self.data.nodes()
    }

    fn name_of(&self, id: u32) -> &str {
        let record = &self.nodes()[id as usize];
        let start = record.name_off as usize;
        let end = start + record.name_len as usize;
        std::str::from_utf8(&self.data.names()[start..end]).unwrap_or("")
    }

    #[inline]
    pub fn size_of(&self, id: u32) -> u64 {
        let overlay = self.overlay.read().unwrap_or_else(|error| error.into_inner());
        overlay.size_of(id, self.nodes()[id as usize].size)
    }

    fn is_deleted(&self, id: u32) -> bool {
        self.overlay
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .is_removed(id)
    }

    /// Exclusive end of the subtree rooted at id, derived from the sibling chain.
    fn subtree_end(&self, id: u32) -> u32 {
        let nodes = self.nodes();
        let count = nodes.len() as u32;
        let mut cursor = id;
        loop {
            let next = nodes[cursor as usize].next_sibling;
            if next != NULL {
                return next;
            }
            let parent = nodes[cursor as usize].parent;
            if parent == NULL {
                return count;
            }
            cursor = parent;
        }
    }

    pub fn path_of(&self, id: u32) -> PathBuf {
        let nodes = self.nodes();
        let mut parts: Vec<&str> = Vec::new();
        let mut cursor = id;
        while cursor != NULL {
            parts.push(self.name_of(cursor));
            cursor = nodes[cursor as usize].parent;
        }
        parts.reverse();
        let mut path = PathBuf::from(parts.first().copied().unwrap_or(""));
        for part in parts.iter().skip(1) {
            path.push(part);
        }
        path
    }

    pub fn path_string(&self, id: u32) -> String {
        self.path_of(id).to_string_lossy().into_owned()
    }

    fn child_ids(&self, id: u32) -> Vec<u32> {
        let nodes = self.nodes();
        let overlay = self.overlay.read().unwrap_or_else(|error| error.into_inner());
        let mut ids = Vec::new();
        let mut cursor = nodes[id as usize].first_child;
        while cursor != NULL {
            if !overlay.is_removed(cursor) {
                ids.push(cursor);
            }
            cursor = nodes[cursor as usize].next_sibling;
        }
        ids
    }

    fn child_info(&self, overlay: &Overlay, id: u32) -> ChildInfo {
        let record = &self.nodes()[id as usize];
        let kind = if record.is_dir() {
            record.dominant
        } else {
            record.kind
        };
        ChildInfo {
            id,
            name: self.name_of(id).to_string(),
            size: overlay.size_of(id, record.size),
            is_dir: record.is_dir(),
            child_count: record.child_count,
            kind: kind_key(kind).to_string(),
        }
    }

    pub fn children_page(&self, id: u32, limit: usize) -> Result<ChildrenPage, String> {
        if id as usize >= self.nodes().len() {
            return Err("无效的节点".to_string());
        }
        let overlay = self.overlay.read().unwrap_or_else(|error| error.into_inner());
        let mut ids = self.child_ids(id);
        let total_children = ids.len() as u32;
        let nodes = self.nodes();
        ids.sort_by_key(|&child| std::cmp::Reverse(overlay.size_of(child, nodes[child as usize].size)));

        let mut omitted = 0u32;
        let mut omitted_size = 0u64;
        if ids.len() > limit {
            for &child in &ids[limit..] {
                omitted += 1;
                omitted_size = omitted_size.saturating_add(overlay.size_of(child, nodes[child as usize].size));
            }
            ids.truncate(limit);
        }

        Ok(ChildrenPage {
            parent: id,
            children: ids
                .iter()
                .map(|&child| self.child_info(&overlay, child))
                .collect(),
            omitted,
            omitted_size,
            total_children,
        })
    }

    pub fn children_batch(&self, ids: &[u32], limit: usize) -> Vec<BatchEntry> {
        let node_count = self.nodes().len();
        ids.iter()
            .filter(|&&id| (id as usize) < node_count)
            .map(|&id| BatchEntry {
                id,
                children: self
                    .children_page(id, limit)
                    .map(|page| page.children)
                    .unwrap_or_default(),
            })
            .collect()
    }

    pub fn node_detail(&self, id: u32) -> Result<NodeDetail, String> {
        if id as usize >= self.nodes().len() {
            return Err("无效的节点".to_string());
        }
        let record = &self.nodes()[id as usize];
        let kind = if record.is_dir() {
            record.dominant
        } else {
            record.kind
        };
        Ok(NodeDetail {
            id,
            name: self.name_of(id).to_string(),
            path: self.path_string(id),
            parent: record.parent,
            size: self.size_of(id),
            is_dir: record.is_dir(),
            kind: kind_key(kind).to_string(),
            child_count: record.child_count,
        })
    }

    /// Ancestor chain from the scan root down to the given node.
    pub fn chain(&self, id: u32) -> Vec<PathEntry> {
        let nodes = self.nodes();
        let mut ids = Vec::new();
        let mut cursor = id;
        while cursor != NULL {
            ids.push(cursor);
            cursor = nodes[cursor as usize].parent;
        }
        ids.reverse();
        ids.iter()
            .map(|&node| PathEntry {
                id: node,
                name: self.name_of(node).to_string(),
                path: self.path_string(node),
            })
            .collect()
    }

    pub fn candidate(&self, id: u32) -> Candidate {
        let record = &self.nodes()[id as usize];
        let kind = if record.is_dir() {
            record.dominant
        } else {
            record.kind
        };
        let path = self.path_of(id);
        let parent_path = path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root_path.to_string_lossy().into_owned());
        Candidate {
            id,
            name: self.name_of(id).to_string(),
            path: path.to_string_lossy().into_owned(),
            parent: record.parent,
            parent_path,
            size: self.size_of(id),
            is_dir: record.is_dir(),
            kind: kind_key(kind).to_string(),
            child_count: record.child_count,
        }
    }

    pub fn top_files(&self, limit: usize) -> Vec<Candidate> {
        let nodes = self.nodes();
        let overlay = self.overlay.read().unwrap_or_else(|error| error.into_inner());
        let mut files: Vec<u32> = Vec::new();
        for (index, record) in nodes.iter().enumerate() {
            if record.is_dir() || overlay.is_removed(index as u32) {
                continue;
            }
            if overlay.size_of(index as u32, record.size) == 0 {
                continue;
            }
            files.push(index as u32);
        }
        files.sort_by_key(|&id| std::cmp::Reverse(overlay.size_of(id, nodes[id as usize].size)));
        files.truncate(limit);
        drop(overlay);
        files.iter().map(|&id| self.candidate(id)).collect()
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<Candidate> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let nodes = self.nodes();
        let overlay = self.overlay.read().unwrap_or_else(|error| error.into_inner());
        let mut hits: Vec<u32> = Vec::new();
        for (index, record) in nodes.iter().enumerate() {
            let id = index as u32;
            if overlay.is_removed(id) {
                continue;
            }
            if overlay.size_of(id, record.size) == 0 {
                continue;
            }
            if self.name_of(id).to_lowercase().contains(&needle) {
                hits.push(id);
            }
        }
        hits.sort_by_key(|&id| std::cmp::Reverse(overlay.size_of(id, nodes[id as usize].size)));
        hits.truncate(limit);
        drop(overlay);
        hits.iter().map(|&id| self.candidate(id)).collect()
    }

    pub fn summary(&self) -> Summary {
        let nodes = self.nodes();
        let overlay = self.overlay.read().unwrap_or_else(|error| error.into_inner());
        let mut total_files = 0u64;
        let mut total_dirs = 0u64;
        let mut kinds: Vec<KindStat> = (0..10)
            .map(|index| KindStat {
                key: kind_key(index as u8).to_string(),
                size: 0,
                count: 0,
            })
            .collect();

        for (index, record) in nodes.iter().enumerate() {
            let id = index as u32;
            if overlay.is_removed(id) {
                continue;
            }
            if record.is_dir() {
                if id != self.root {
                    total_dirs += 1;
                }
            } else {
                let size = overlay.size_of(id, record.size);
                total_files += 1;
                let kind = record.kind as usize;
                if kind < kinds.len() {
                    kinds[kind].size = kinds[kind].size.saturating_add(size);
                    kinds[kind].count += 1;
                }
            }
        }

        Summary {
            root: self.root_path.to_string_lossy().into_owned(),
            root_id: self.root,
            total_size: overlay.size_of(self.root, nodes[self.root as usize].size),
            total_files,
            total_dirs,
            elapsed_ms: self.elapsed_ms,
            errors: self.errors,
            kinds,
        }
    }

    /// Moves the node subtree to the OS recycle bin and updates every ancestor size.
    pub fn remove_subtree(&self, id: u32) -> Result<(), String> {
        if id == self.root {
            return Err("不能删除扫描根目录".to_string());
        }
        if id as usize >= self.nodes().len() || self.is_deleted(id) {
            return Err("无效的节点".to_string());
        }
        let path = self.path_of(id);
        if path.as_os_str().is_empty() {
            return Err("无效的路径".to_string());
        }
        trash::delete(&path).map_err(|error| format!("无法移动到回收站：{}", error))?;
        self.mark_deleted(id);
        Ok(())
    }

    fn mark_deleted(&self, id: u32) {
        let freed = self.size_of(id);
        let end = self.subtree_end(id);
        let nodes = self.nodes();
        let mut overlay = self.overlay.write().unwrap_or_else(|error| error.into_inner());
        overlay.removed.push((id, end));
        let mut cursor = nodes[id as usize].parent;
        while cursor != NULL {
            let entry = overlay.shaved.entry(cursor).or_insert(0);
            *entry = entry.saturating_add(freed);
            cursor = nodes[cursor as usize].parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{category_of, KIND_OTHER};

    fn file(name: &str, size: u64) -> RawNode {
        let kind = category_of(name);
        RawNode {
            name: name.into(),
            size,
            is_dir: false,
            kind,
            dominant: kind,
            children: Vec::new(),
        }
    }

    fn dir(name: &str, children: Vec<RawNode>) -> RawNode {
        let size = children.iter().map(|child| child.size).sum();
        let mut dominant = KIND_OTHER;
        let mut best = 0;
        for child in &children {
            if child.size > best {
                best = child.size;
                dominant = child.dominant;
            }
        }
        RawNode {
            name: name.into(),
            size,
            is_dir: true,
            kind: KIND_OTHER,
            dominant,
            children,
        }
    }

    fn sample_tree() -> ScanTree {
        let root = dir(
            "C:\\tree-root",
            vec![
                dir("small", vec![file("note.txt", 100)]),
                dir("big", vec![file("movie.mp4", 900), file("clip.mkv", 200)]),
                file("loose.bin", 50),
            ],
        );
        ScanTree::build(root, PathBuf::from("C:\\tree-root"), 2, 35)
    }

    #[test]
    fn node_records_stay_packed() {
        assert_eq!(std::mem::size_of::<NodeRecord>(), 40);
        assert_eq!(std::mem::size_of::<NodeRecord>() % 8, 0);
    }

    #[test]
    fn the_index_is_spilled_to_disk() {
        let tree = sample_tree();
        assert!(tree.data.is_mapped(), "scan results should be mmap backed");
        assert_eq!(tree.nodes().len(), 7);
    }

    #[test]
    fn sizes_and_children_are_aggregated() {
        let tree = sample_tree();
        let page = tree.children_page(tree.root, 10).unwrap();
        assert_eq!(page.total_children, 3);
        assert_eq!(page.children[0].name, "big");
        assert_eq!(page.children[0].size, 1100);
        assert_eq!(page.children[1].name, "small");
        assert_eq!(page.children[1].kind, "document");
        assert!(page.children[0].is_dir);
        assert_eq!(page.children[0].child_count, 2);
    }

    #[test]
    fn paths_are_rebuilt_from_the_mapped_names() {
        let tree = sample_tree();
        let page = tree.children_page(tree.root, 10).unwrap();
        let big = page.children[0].id;
        let inner = tree.children_page(big, 10).unwrap();
        assert_eq!(
            tree.path_string(inner.children[0].id),
            "C:\\tree-root\\big\\movie.mp4"
        );
        let detail = tree.node_detail(inner.children[0].id).unwrap();
        assert_eq!(detail.parent, big);
        assert!(!detail.is_dir);
    }

    #[test]
    fn search_and_top_files_respect_size_order() {
        let tree = sample_tree();
        let hits = tree.search("movie", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].size, 900);

        let top = tree.top_files(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].name, "movie.mp4");
        assert_eq!(top[1].name, "clip.mkv");
    }

    #[test]
    fn summary_counts_files_dirs_and_kinds() {
        let tree = sample_tree();
        let summary = tree.summary();
        assert_eq!(summary.total_size, 1250);
        assert_eq!(summary.total_files, 4);
        assert_eq!(summary.total_dirs, 2);
        assert_eq!(summary.root, "C:\\tree-root");
        let video = summary.kinds.iter().find(|kind| kind.key == "video").unwrap();
        assert_eq!(video.size, 1100);
        assert_eq!(video.count, 2);
    }

    #[test]
    fn children_pages_are_limited_and_reported() {
        let mut kids = Vec::new();
        for index in 0..40 {
            kids.push(file(&format!("f{}.bin", index), (index as u64 + 1) * 10));
        }
        let tree = ScanTree::build(dir("C:\\many", kids), PathBuf::from("C:\\many"), 0, 1);
        let page = tree.children_page(tree.root, 10).unwrap();
        assert_eq!(page.children.len(), 10);
        assert_eq!(page.total_children, 40);
        assert_eq!(page.omitted, 30);
        assert_eq!(page.children[0].size, 400);
        assert!(page.omitted_size > 0);
    }

    #[test]
    fn deleted_subtrees_disappear_from_listings() {
        let tree = sample_tree();
        let page = tree.children_page(tree.root, 10).unwrap();
        let small = page.children[1].id;
        tree.mark_deleted(small);
        let after = tree.children_page(tree.root, 10).unwrap();
        assert_eq!(after.total_children, 2);
        assert_eq!(after.children[0].name, "big");
        assert_eq!(after.children[1].name, "loose.bin");
        assert_eq!(tree.summary().total_files, 3);
    }

    #[test]
    fn deleting_a_directory_hides_its_descendants() {
        let tree = sample_tree();
        let page = tree.children_page(tree.root, 10).unwrap();
        let big = page.children[0].id;
        tree.mark_deleted(big);

        assert_eq!(tree.size_of(tree.root), 150);
        assert!(tree.children_page(big, 10).unwrap().children.is_empty());
        assert!(tree.search("movie", 10).is_empty());
        assert_eq!(tree.summary().total_files, 2);
        assert_eq!(tree.summary().total_dirs, 1);
        let top = tree.top_files(5);
        assert_eq!(top[0].name, "note.txt");
    }

    /// Diagnostic only: builds a million node index and then parks, so an
    /// external sampler can read the resident size. Run with
    ///   cargo test --lib -- --ignored --nocapture large_index_footprint
    #[test]
    #[ignore]
    fn large_index_footprint() {
        let dirs = 1000;
        let per_dir = 1000;
        let mut root_children = Vec::with_capacity(dirs);
        for d in 0..dirs {
            let mut kids = Vec::with_capacity(per_dir);
            for f in 0..per_dir {
                kids.push(file(&format!("payload-{:04}-{:04}.bin", d, f), 4096));
            }
            root_children.push(dir(&format!("dir-{:04}", d), kids));
        }
        let raw = dir("C:\\\\synthetic", root_children);
        println!("raw tree built");
        let started = std::time::Instant::now();
        let tree = ScanTree::build(raw, PathBuf::from("C:\\\\synthetic"), 0, 0);
        println!(
            "spilled {} nodes in {:?}, mapped={}",
            tree.nodes().len(),
            started.elapsed(),
            tree.data.is_mapped()
        );
        // Give the sampler time to read the resident size of the finished index.
        std::thread::sleep(std::time::Duration::from_secs(12));
        assert_eq!(tree.children_page(tree.root, 3).unwrap().children.len(), 3);
    }

    #[test]
    fn subtree_extents_follow_the_sibling_chain() {
        let tree = sample_tree();
        let page = tree.children_page(tree.root, 10).unwrap();
        let small = page.children[1].id;
        let big = page.children[0].id;
        let loose = page.children[2].id;
        assert_eq!(tree.subtree_end(small), big);
        assert_eq!(tree.subtree_end(big), loose);
        assert_eq!(tree.subtree_end(loose), 7);
        assert_eq!(tree.subtree_end(tree.root), 7);
    }
}
