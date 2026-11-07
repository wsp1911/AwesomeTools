const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return Math.round(value).toString() + " B";
  const digits = value >= 100 ? 0 : 1;
  return value.toFixed(digits) + " " + UNITS[unit];
}

export function formatCount(value: number): string {
  if (!Number.isFinite(value)) return "0";
  return Math.round(value).toLocaleString("en-US");
}

export function formatPercent(part: number, total: number): string {
  if (!total || total <= 0) return "0%";
  const ratio = (part / total) * 100;
  if (ratio > 0 && ratio < 0.1) return "<0.1%";
  return ratio.toFixed(ratio >= 10 ? 0 : 1) + "%";
}

export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "0 秒";
  const seconds = ms / 1000;
  if (seconds < 1) return Math.round(ms) + " 毫秒";
  if (seconds < 60) return seconds.toFixed(seconds < 10 ? 1 : 0) + " 秒";
  const minutes = Math.floor(seconds / 60);
  const rest = Math.round(seconds % 60);
  return minutes + " 分 " + rest + " 秒";
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
}

export function truncateMiddle(text: string, max: number): string {
  if (text.length <= max) return text;
  const head = Math.ceil((max - 3) / 2);
  const tail = Math.floor((max - 3) / 2);
  return text.slice(0, head) + "..." + text.slice(text.length - tail);
}
