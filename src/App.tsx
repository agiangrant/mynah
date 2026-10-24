import { useEffect, useState } from "react";
import { PAGES, Sidebar, type Page } from "./components/Sidebar";
import { Toaster } from "./components/Toast";
import { getEngineStatus, listModels } from "./lib/ipc";
import { loadSystemInfo } from "./lib/platform";
import { useEvent, useSettings } from "./lib/store";
import type { EngineStatus, ModelInfo } from "./lib/types";
import { DictationPage } from "./pages/Dictation";
import { GeneralPage } from "./pages/General";
import { HistoryPage } from "./pages/History";
import { MeetingsPage } from "./pages/Meetings";
import { ModelsPage } from "./pages/Models";
import { ShortcutsPage } from "./pages/Shortcuts";
import { VocabularyPage } from "./pages/Vocabulary";

const PAGE_IDS = new Set<string>(PAGES.map((p) => p.id));

function readInitialPage(): Page {
  const h = window.location.hash.replace(/^#\/?/, "");
  return PAGE_IDS.has(h) ? (h as Page) : "general";
}

export default function App() {
  const [page, setPage] = useState<Page>(readInitialPage);
  const [engine, setEngine] = useState<EngineStatus | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const { settings } = useSettings();

  useEffect(() => {
    void loadSystemInfo();
    getEngineStatus().then(setEngine).catch(() => undefined);
    listModels().then(setModels).catch(() => undefined);
  }, []);
  useEvent("engine-status", setEngine);
  useEvent("models-changed", () => listModels().then(setModels).catch(() => undefined));
  useEvent("navigate", ({ page: p }) => {
    if (PAGE_IDS.has(p)) setPage(p as Page);
  });

  // theme
  useEffect(() => {
    const t = settings?.general.theme ?? "system";
    const el = document.documentElement;
    if (t === "system") el.removeAttribute("data-theme");
    else el.setAttribute("data-theme", t);
  }, [settings?.general.theme]);

  useEffect(() => {
    window.location.hash = page;
    document.querySelector(".content")?.scrollTo({ top: 0 });
  }, [page]);

  return (
    <div className="app">
      <Sidebar page={page} onNavigate={setPage} engine={engine} models={models} />
      <main className="content">
        <div className="drag-strip" data-tauri-drag-region />
        {page === "general" ? <GeneralPage /> : null}
        {page === "models" ? <ModelsPage /> : null}
        {page === "shortcuts" ? <ShortcutsPage /> : null}
        {page === "dictation" ? <DictationPage /> : null}
        {page === "vocabulary" ? <VocabularyPage /> : null}
        {page === "meetings" ? <MeetingsPage /> : null}
        {page === "history" ? <HistoryPage /> : null}
      </main>
      <Toaster />
    </div>
  );
}
