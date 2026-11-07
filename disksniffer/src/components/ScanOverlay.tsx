import type { ProgressPayload } from "../lib/types";
import { formatBytes, formatCount } from "../lib/format";

interface ScanOverlayProps {
  progress: ProgressPayload | null;
  onCancel: () => void;
}

export function ScanOverlay(props: ScanOverlayProps) {
  const progress = props.progress;
  return (
    <div className="scan-overlay">
      <div className="scan-card">
        <div className="scan-head">
          <span className="spinner" />
          正在扫描
        </div>
        <div className="scan-path" title={progress ? progress.current : ""}>
          {progress && progress.current ? progress.current : "正在枚举目录…"}
        </div>
        <div className="scan-stats">
          <div>
            <span>已扫描文件</span>
            <strong>{formatCount(progress ? progress.files : 0)}</strong>
          </div>
          <div>
            <span>文件夹</span>
            <strong>{formatCount(progress ? progress.dirs : 0)}</strong>
          </div>
          <div>
            <span>累计大小</span>
            <strong>{formatBytes(progress ? progress.bytes : 0)}</strong>
          </div>
        </div>
        <div className="scan-bar">
          <div className="scan-bar-fill" />
        </div>
        <button className="btn btn-stop" onClick={props.onCancel}>
          停止扫描
        </button>
      </div>
    </div>
  );
}
