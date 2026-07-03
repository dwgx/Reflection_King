import { Clipboard, ExternalLink, Loader2, X } from "lucide-react";
import type { ConfirmDialogState, FilePreviewState, NotificationItem } from "../types";
import { formatBytes } from "../lib/format";
import { ActionLink, Button } from "./primitives";

export function NotificationStack(props: { items: NotificationItem[]; onClose: (id: number) => void }) {
  return (
    <div className="notification-stack" aria-live="polite">
      {props.items.map((item) => (
        <div key={item.id} className={`notification ${item.tone}`}>
          <span>{item.text}</span>
          <button type="button" onClick={() => props.onClose(item.id)} aria-label="关闭通知">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}

export function FilePreviewModal(props: { preview: FilePreviewState; onClose: () => void; onCopy: () => void }) {
  const type = props.preview.contentType.toLowerCase();
  const isFrame = type.includes("html")
    || type.includes("json")
    || type.includes("text/")
    || type.includes("javascript")
    || type.includes("css")
    || type.includes("xml");
  const isImage = type.startsWith("image/");

  return (
    <div className="file-preview-overlay" role="dialog" aria-modal="true" aria-label="文件预览">
      <section className="file-preview-modal">
        <div className="file-preview-head">
          <div>
            <strong>{props.preview.title}</strong>
            <span>{props.preview.contentType || "application/octet-stream"} / {formatBytes(props.preview.bytes)}</span>
          </div>
          <div className="panel-actions">
            <Button type="button" variant="secondary" onClick={props.onCopy}>
              <Clipboard size={16} /> 复制接口
            </Button>
            <ActionLink href={props.preview.blobUrl} variant="secondary" download={props.preview.title}>
              <ExternalLink size={16} /> 下载
            </ActionLink>
            <Button type="button" variant="secondary" onClick={props.onClose}>
              <X size={16} /> 关闭
            </Button>
          </div>
        </div>
        <div className="file-preview-body">
          {isImage ? (
            <img src={props.preview.blobUrl} alt="" />
          ) : isFrame ? (
            <iframe
              title={props.preview.title}
              src={props.preview.blobUrl}
              sandbox=""
              referrerPolicy="no-referrer"
            />
          ) : (
            <div className="file-preview-empty">
              <strong>此文件类型不适合内嵌预览</strong>
              <span>使用下载按钮保存后用本地工具查看。</span>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

export function ConfirmDialog(props: {
  state: ConfirmDialogState | null;
  busy: boolean;
  onClose: () => void;
}) {
  if (!props.state) return null;
  return (
    <div className="modal-backdrop" role="presentation">
      <section className="confirm-modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
        <div>
          <h2 id="confirm-title">{props.state.title}</h2>
          <p>{props.state.message}</p>
        </div>
        <div className="modal-actions">
          <Button type="button" variant="secondary" onClick={props.onClose} disabled={props.busy}>
            取消
          </Button>
          <Button
            type="button"
            className={props.state.danger ? "danger-button" : ""}
            disabled={props.busy}
            onClick={async () => {
              await props.state?.onConfirm();
              props.onClose();
            }}
          >
            {props.busy ? <Loader2 className="animate-spin" size={16} /> : null}
            {props.state.confirmLabel}
          </Button>
        </div>
      </section>
    </div>
  );
}
