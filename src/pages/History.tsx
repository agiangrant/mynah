import { useCallback, useEffect, useState } from "react";
import { ChevronRight, Clock, Copy, FolderOpen, Mic, Search, Trash2, Users } from "lucide-react";
import { Button } from "../components/Button";
import { EmptyState } from "../components/EmptyState";
import { Group, LoadingPage, PageHead } from "../components/Field";
import { toast } from "../components/Toast";
import { clearHistory, copyText, deleteHistoryEntry, getHistory, revealPath } from "../lib/ipc";
import { absoluteTime, durationShort, firstLine, relativeTime } from "../lib/format";
import { useDebounced, useEvent } from "../lib/store";
import type { HistoryEntry } from "../lib/types";
import { Transcript } from "./Meetings";

export function HistoryPage() {
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [query, setQuery] = useState("");
  const q = useDebounced(query, 200);
  const [open, setOpen] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  const refresh = useCallback(() => {
    getHistory(q, 200, 0)
      .then(setEntries)
      .catch((e) => {
        setEntries([]);
        toast(`Couldn't load history: ${String(e)}`, "error");
      });
  }, [q]);

  useEffect(refresh, [refresh]);
  useEvent("history-updated", refresh);

  const act = (fn: () => Promise<unknown>, ok?: string) =>
    fn()
      .then(() => ok && toast(ok, "success"))
      .catch((e) => toast(String(e), "error"));

  if (!entries) return <LoadingPage />;

  return (
    <div className="page">
      <PageHead title="History" subtitle="Everything you've dictated and every meeting transcript, stored only on this computer." />

      <div className="toolbar">
        <label className="search">
          <Search size={14} />
          <input placeholder="Search transcripts" value={query} onChange={(e) => setQuery(e.target.value)} />
        </label>
        <span className="spacer" />
        {entries.length > 0 ? (
          confirmClear ? (
            <span className="confirm">
              Delete everything?
              <Button size="sm" variant="danger" onClick={() => void act(clearHistory, "History cleared").then(() => setConfirmClear(false))}>
                Delete all
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setConfirmClear(false)}>
                Keep
              </Button>
            </span>
          ) : (
            <Button size="sm" variant="ghost" onClick={() => setConfirmClear(true)}>
              Clear all
            </Button>
          )
        ) : null}
      </div>

      <Group>
        {entries.length === 0 ? (
          q ? (
            <EmptyState title={`Nothing matches “${q}”`} />
          ) : (
            <EmptyState
              icon={<Clock size={22} />}
              title="Nothing here yet"
              body="Hold your dictation key and say something. It will show up here along with how long it took."
            />
          )
        ) : (
          entries.map((e) => {
            const isOpen = open === e.id;
            const meeting = e.kind === "meeting";
            return (
              <div key={e.id} className={`hist ${isOpen ? "open" : ""}`}>
                <div className="hist-row">
                  <button
                    type="button"
                    className="hist-row"
                    onClick={() => setOpen(isOpen ? null : e.id)}
                    aria-expanded={isOpen}
                  >
                    <ChevronRight size={14} className="chevron" />
                    <span className={`hist-icon ${meeting ? "meeting" : ""}`}>
                      {meeting ? <Users size={14} /> : <Mic size={14} />}
                    </span>
                    <span className="hist-main">
                      <div className="hist-text">{e.title ?? firstLine(e.text) ?? "(empty)"}</div>
                      <div className="hist-meta" title={absoluteTime(e.created_at)}>
                        {relativeTime(e.created_at)}
                        {" · "}
                        {durationShort(e.duration_ms)}
                        {meeting && e.app ? ` · ${e.app}` : ""}
                        {e.model_id ? ` · ${e.model_id}` : ""}
                      </div>
                    </span>
                  </button>
                  <span className="hist-actions">
                    <Button size="sm" variant="ghost" className="btn-icon" aria-label="Copy text" title="Copy" onClick={() => void act(() => copyText(e.text), "Copied")}>
                      <Copy size={13} />
                    </Button>
                    {e.saved_path ? (
                      <Button size="sm" variant="ghost" className="btn-icon" aria-label="Show file" title="Show file" onClick={() => void act(() => revealPath(e.saved_path!))}>
                        <FolderOpen size={13} />
                      </Button>
                    ) : null}
                    <Button size="sm" variant="ghost" className="btn-icon" aria-label="Delete" title="Delete" onClick={() => void act(() => deleteHistoryEntry(e.id))}>
                      <Trash2 size={13} />
                    </Button>
                  </span>
                </div>
                {isOpen ? (
                  <div className="hist-body">
                    {meeting && e.segments && e.segments.length > 0 ? <Transcript segments={e.segments} /> : e.text || <span className="faint">Nothing was recognised.</span>}
                  </div>
                ) : null}
              </div>
            );
          })
        )}
      </Group>
    </div>
  );
}
