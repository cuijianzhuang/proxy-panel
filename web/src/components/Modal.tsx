import { ReactNode, useEffect } from "react";

/*
 * Tiny modal — no portal, no animation. Closes on Escape and on overlay click.
 * `size` toggles the max-width so wider forms (the smart listener form) don't
 * feel cramped on big screens.
 */
export type ModalSize = "md" | "lg" | "xl";
const SIZE_CLASS: Record<ModalSize, string> = {
  md: "max-w-xl",
  lg: "max-w-2xl",
  xl: "max-w-3xl",
};

export function Modal({
  open,
  onClose,
  title,
  size = "md",
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  size?: ModalSize;
  children: ReactNode;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(15, 23, 42, 0.45)" }}
      onClick={onClose}
    >
      <div
        className={`card w-full ${SIZE_CLASS[size]} max-h-[85vh] overflow-y-auto`}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="px-5 py-3 border-b flex items-center justify-between"
          style={{ borderColor: "var(--border)" }}
        >
          <h2 className="font-semibold">{title}</h2>
          <button onClick={onClose} className="btn btn-ghost" aria-label="close">
            ✕
          </button>
        </div>
        <div className="p-5">{children}</div>
      </div>
    </div>
  );
}
