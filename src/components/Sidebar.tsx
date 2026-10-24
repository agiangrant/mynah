import {
  BookA,
  Clock,
  Keyboard,
  Mic,
  Package,
  Settings2,
  Users,
} from "lucide-react";
import type { ReactNode } from "react";
import type { EngineStatus, ModelInfo } from "../lib/types";
import { useSystemInfo } from "../lib/platform";

export type Page =
  | "general"
  | "models"
  | "shortcuts"
  | "dictation"
  | "vocabulary"
  | "meetings"
  | "history";

export const PAGES: { id: Page; label: string; icon: ReactNode }[] = [
  { id: "general", label: "General", icon: <Settings2 size={16} /> },
  { id: "models", label: "Models", icon: <Package size={16} /> },
  { id: "shortcuts", label: "Shortcuts", icon: <Keyboard size={16} /> },
  { id: "dictation", label: "Dictation", icon: <Mic size={16} /> },
  { id: "vocabulary", label: "Vocabulary", icon: <BookA size={16} /> },
  { id: "meetings", label: "Meetings", icon: <Users size={16} /> },
  { id: "history", label: "History", icon: <Clock size={16} /> },
];

export function Sidebar({
  page,
  onNavigate,
  engine,
  models,
}: {
  page: Page;
  onNavigate: (p: Page) => void;
  engine: EngineStatus | null;
  models: ModelInfo[];
}) {
  const info = useSystemInfo();
  const modelName = engine?.model_id
    ? (models.find((m) => m.id === engine.model_id)?.name ?? engine.model_id)
    : null;
  const state = engine?.state ?? "unloaded";
  const statusText =
    state === "ready"
      ? modelName ?? "Ready"
      : state === "loading"
        ? `Loading ${modelName ?? "model"}…`
        : state === "error"
          ? "Engine error"
          : "No model loaded";

  return (
    <aside className="sidebar">
      <div className="drag-strip" data-tauri-drag-region />
      <div className="brand" data-tauri-drag-region>
        <img src="/icon.png" alt="" width={22} height={22} className="brand-mark" />
        <span className="brand-name">Mynah</span>
      </div>
      <nav className="nav" aria-label="Settings">
        {PAGES.map((p) => (
          <button
            key={p.id}
            className={`nav-item ${p.id === page ? "active" : ""}`}
            onClick={() => onNavigate(p.id)}
            aria-current={p.id === page ? "page" : undefined}
          >
            {p.icon}
            <span>{p.label}</span>
          </button>
        ))}
      </nav>
      <div className="sidebar-foot">
        <button className="engine-pill" onClick={() => onNavigate("models")} title={engine?.message ?? statusText}>
          <span className={`status-dot status-${state}`} />
          <span className="engine-pill-text">{statusText}</span>
        </button>
        {info ? <span className="version">v{info.app_version}</span> : null}
      </div>
    </aside>
  );
}
