import type { NodeSelection } from "../lib/types";

interface ContextMenuProps {
  x: number;
  y: number;
  node: NodeSelection;
  onClose: () => void;
  onOpen: (path: string) => void;
  onReveal: (path: string) => void;
  onCopyPath: (path: string) => void;
  onFocus: (node: NodeSelection) => void;
  onTrash: (node: NodeSelection) => void;
}

export function ContextMenu(props: ContextMenuProps) {
  const left = Math.min(props.x, window.innerWidth - 232);
  const top = Math.min(props.y, window.innerHeight - 200);
  return (
    <>
      <div className="ctx-backdrop" onClick={props.onClose} onContextMenu={(event) => event.preventDefault()} />
      <div className="ctx-menu" style={{ left: left, top: top }}>
        <div className="ctx-head" title={props.node.name}>
          {props.node.name}
        </div>
        {props.node.isDir ? (
          <>
            <button
              className="ctx-item"
              onClick={() => {
                props.onFocus(props.node);
                props.onClose();
              }}
            >
              在树图中打开
            </button>
            <button
              className="ctx-item"
              onClick={() => {
                props.onOpen(props.node.path);
                props.onClose();
              }}
            >
              在资源管理器中打开
            </button>
          </>
        ) : (
          <button
            className="ctx-item"
            onClick={() => {
              props.onReveal(props.node.path);
              props.onClose();
            }}
          >
            在资源管理器中定位
          </button>
        )}
        <button
          className="ctx-item"
          onClick={() => {
            props.onCopyPath(props.node.path);
            props.onClose();
          }}
        >
          复制完整路径
        </button>
        <div className="ctx-divider" />
        <button
          className="ctx-item danger"
          onClick={() => {
            props.onTrash(props.node);
            props.onClose();
          }}
        >
          移到回收站
        </button>
      </div>
    </>
  );
}
