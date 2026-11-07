import type { ReactNode } from "react";

interface ConfirmDialogProps {
  title: string;
  message: ReactNode;
  detail?: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog(props: ConfirmDialogProps) {
  return (
    <div className="modal-backdrop" onClick={props.onCancel}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <div className="modal-title">{props.title}</div>
        <div className="modal-message">{props.message}</div>
        {props.detail ? (
          <div className="modal-detail" title={props.detail}>
            {props.detail}
          </div>
        ) : null}
        <div className="modal-actions">
          <button className="btn btn-ghost" onClick={props.onCancel}>
            取消
          </button>
          <button
            className={"btn " + (props.danger ? "btn-danger" : "btn-primary")}
            onClick={props.onConfirm}
          >
            {props.confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
