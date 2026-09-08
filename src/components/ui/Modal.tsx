import { useEffect, type ReactNode } from "react";
import { X, AlertTriangle } from "lucide-react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/format";
import { Button } from "./Button";

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
}

export function Modal({ open, onClose, title, children, footer }: ModalProps) {
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;
  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-[2px]"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="w-[440px] max-w-[90vw] rounded-2xl bg-white dark:bg-zinc-900 shadow-xl border border-zinc-200 dark:border-zinc-800 animate-[modal-in_.15s_ease-out]">
        <div className="flex items-center justify-between px-5 py-4 border-b border-zinc-100 dark:border-zinc-800">
          <h3 className="text-sm font-semibold">{title}</h3>
          <button
            onClick={onClose}
            className="p-1 rounded-md text-zinc-400 hover:bg-zinc-100 dark:hover:bg-zinc-800 cursor-pointer"
          >
            <X className="size-4" />
          </button>
        </div>
        <div className="px-5 py-4 text-sm text-zinc-600 dark:text-zinc-300">{children}</div>
        {footer && (
          <div className="flex justify-end gap-2 px-5 py-3.5 border-t border-zinc-100 dark:border-zinc-800">
            {footer}
          </div>
        )}
      </div>
      <style>{`@keyframes modal-in { from { opacity: 0; transform: scale(.96) } to { opacity: 1; transform: scale(1) } }`}</style>
    </div>,
    document.body,
  );
}

interface ConfirmProps {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  message: ReactNode;
  danger?: boolean;
  confirmText?: string;
  loading?: boolean;
}

export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  message,
  danger = false,
  confirmText = "确认",
  loading = false,
}: ConfirmProps) {
  return (
    <Modal
      open={open}
      onClose={loading ? () => {} : onClose}
      title={
        <span className="flex items-center gap-2">
          <AlertTriangle
            className={cn("size-4", danger ? "text-red-500" : "text-amber-500")}
          />
          {title}
        </span>
      }
      footer={
        <>
          <Button onClick={onClose} disabled={loading}>
            取消
          </Button>
          <Button
            variant={danger ? "danger" : "primary"}
            onClick={onConfirm}
            loading={loading}
          >
            {confirmText}
          </Button>
        </>
      }
    >
      {message}
    </Modal>
  );
}
