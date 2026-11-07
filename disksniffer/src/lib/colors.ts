export interface KindMeta {
  label: string;
  color: string;
}

export const KIND_META: Record<string, KindMeta> = {
  video: { label: "视频", color: "#e06054" },
  image: { label: "图片", color: "#dfa14a" },
  audio: { label: "音频", color: "#a98cf0" },
  document: { label: "文档", color: "#4f9df0" },
  code: { label: "代码", color: "#37b3a8" },
  archive: { label: "压缩包", color: "#c8ac3a" },
  executable: { label: "程序", color: "#8a7cf2" },
  system: { label: "系统", color: "#67788e" },
  font: { label: "字体", color: "#5cb96b" },
  other: { label: "其他", color: "#78859a" },
};

export const KIND_ORDER = [
  "video",
  "image",
  "audio",
  "document",
  "code",
  "archive",
  "executable",
  "system",
  "font",
  "other",
];

export function kindMeta(kind: string): KindMeta {
  return KIND_META[kind] ?? KIND_META.other;
}

export function kindLabel(kind: string): string {
  return kindMeta(kind).label;
}

export function hexToRgb(hex: string): [number, number, number] {
  const value = hex.replace("#", "");
  return [
    parseInt(value.slice(0, 2), 16),
    parseInt(value.slice(2, 4), 16),
    parseInt(value.slice(4, 6), 16),
  ];
}

export function withAlpha(hex: string, alpha: number): string {
  const [r, g, b] = hexToRgb(hex);
  return "rgba(" + r + ", " + g + ", " + b + ", " + alpha + ")";
}

export interface RectPaint {
  background: string;
  backgroundImage: string;
  boxShadow: string;
  borderColor: string;
}

export function paintRect(kind: string, isDir: boolean, depth: number): RectPaint {
  const color = kindMeta(kind).color;
  const fade = Math.max(0.34, 0.62 - depth * 0.12);
  if (isDir) {
    return {
      background: withAlpha(color, depth === 0 ? 0.2 : 0.13),
      backgroundImage: "none",
      boxShadow:
        "inset 0 0 0 1px " + withAlpha(color, depth === 0 ? 0.45 : 0.28),
      borderColor: withAlpha(color, 0.45),
    };
  }
  return {
    background: withAlpha(color, fade),
    backgroundImage:
      "linear-gradient(180deg, rgba(255,255,255,0.10), rgba(0,0,0,0.16))",
    boxShadow:
      "inset 0 0 0 1px " + withAlpha(color, Math.min(1, fade + 0.16)),
    borderColor: color,
  };
}
