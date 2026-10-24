import { X } from "lucide-react";
import type { ReactNode } from "react";

export function Chip({
  children,
  onRemove,
  tone = "default",
}: {
  children: ReactNode;
  onRemove?: () => void;
  tone?: "default" | "accent" | "muted";
}) {
  return (
    <span className={`chip chip-${tone}`}>
      {children}
      {onRemove ? (
        <button type="button" className="chip-x" onClick={onRemove} aria-label="Remove">
          <X size={12} />
        </button>
      ) : null}
    </span>
  );
}

export function Badge({
  children,
  tone = "default",
}: {
  children: ReactNode;
  tone?: "default" | "accent" | "ok" | "danger" | "warn";
}) {
  return <span className={`badge badge-${tone}`}>{children}</span>;
}
