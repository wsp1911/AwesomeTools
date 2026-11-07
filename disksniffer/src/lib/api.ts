import { invoke } from "@tauri-apps/api/core";
import type {
  BatchEntry,
  Candidate,
  ChildrenPage,
  NodeDetail,
  PathEntry,
  Summary,
} from "./types";

export const api = {
  startScan: (path: string) => invoke<void>("start_scan", { path }),
  cancelScan: () => invoke<void>("cancel_scan"),
  getSummary: () => invoke<Summary | null>("get_summary"),
  getChildren: (nodeId: number, limit = 500) =>
    invoke<ChildrenPage>("get_children", { nodeId, limit }),
  getChildrenBatch: (ids: number[], limit = 64) =>
    invoke<BatchEntry[]>("get_children_batch", { ids, limit }),
  getNode: (nodeId: number) => invoke<NodeDetail>("get_node", { nodeId }),
  getChain: (nodeId: number) => invoke<PathEntry[]>("get_chain", { nodeId }),
  topFiles: (limit = 20) => invoke<Candidate[]>("top_files", { limit }),
  search: (query: string, limit = 150) =>
    invoke<Candidate[]>("search", { query, limit }),
  trashNode: (nodeId: number) => invoke<void>("trash_node", { nodeId }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  listDrives: () => invoke<string[]>("list_drives"),
};

export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    document.body.removeChild(area);
  }
}
