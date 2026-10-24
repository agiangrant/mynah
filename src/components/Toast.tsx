import { useEffect, useState } from "react";
import { AlertCircle, CheckCircle2, Info, X } from "lucide-react";

type Kind = "info" | "success" | "error";
interface ToastItem {
  id: number;
  message: string;
  kind: Kind;
}

let nextId = 1;
let items: ToastItem[] = [];
const listeners = new Set<(items: ToastItem[]) => void>();

function publish() {
  listeners.forEach((l) => l(items));
}

export function toast(message: string, kind: Kind = "info") {
  const id = nextId++;
  items = [...items, { id, message, kind }];
  publish();
  window.setTimeout(() => dismiss(id), kind === "error" ? 6000 : 3200);
}

function dismiss(id: number) {
  items = items.filter((t) => t.id !== id);
  publish();
}

export function Toaster() {
  const [list, setList] = useState<ToastItem[]>(items);
  useEffect(() => {
    listeners.add(setList);
    return () => {
      listeners.delete(setList);
    };
  }, []);
  if (list.length === 0) return null;
  return (
    <div className="toaster" role="status" aria-live="polite">
      {list.map((t) => (
        <div key={t.id} className={`toast toast-${t.kind}`}>
          {t.kind === "error" ? (
            <AlertCircle size={15} />
          ) : t.kind === "success" ? (
            <CheckCircle2 size={15} />
          ) : (
            <Info size={15} />
          )}
          <span className="toast-text">{t.message}</span>
          <button className="toast-close" onClick={() => dismiss(t.id)} aria-label="Dismiss">
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
