export interface KindStat {
  key: string;
  size: number;
  count: number;
}

export interface Summary {
  root: string;
  rootId: number;
  totalSize: number;
  totalFiles: number;
  totalDirs: number;
  elapsedMs: number;
  errors: number;
  kinds: KindStat[];
}

export interface ChildInfo {
  id: number;
  name: string;
  size: number;
  isDir: boolean;
  childCount: number;
  kind: string;
}

export interface ChildrenPage {
  parent: number;
  children: ChildInfo[];
  omitted: number;
  omittedSize: number;
  totalChildren: number;
}

export interface BatchEntry {
  id: number;
  children: ChildInfo[];
}

export interface Candidate {
  id: number;
  name: string;
  path: string;
  parent: number;
  parentPath: string;
  size: number;
  isDir: boolean;
  kind: string;
  childCount: number;
}

export interface NodeDetail {
  id: number;
  name: string;
  path: string;
  parent: number;
  size: number;
  isDir: boolean;
  kind: string;
  childCount: number;
}

export interface PathEntry {
  id: number;
  name: string;
  path: string;
}

export interface ProgressPayload {
  files: number;
  dirs: number;
  bytes: number;
  current: string;
}

export interface NavEntry {
  id: number;
  name: string;
  path: string;
}

export interface NodeSelection {
  id: number;
  name: string;
  path: string;
  size: number;
  isDir: boolean;
  kind: string;
  childCount: number;
}
