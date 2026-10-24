import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { getSettings, onEvent, setSettings as saveSettings } from "./ipc";
import type { EventMap, Settings } from "./types";
import { toast } from "../components/Toast";

// ---------------------------------------------------------------------------
// Settings store: one in-memory copy shared by every component, debounced
// writes to the backend, reconciled with the normalized Settings it returns.
// ---------------------------------------------------------------------------

type Listener = () => void;

let current: Settings | null = null;
let dirty = false;
let flushTimer: number | undefined;
let inflight: Promise<void> | null = null;
let loadStarted = false;
const listeners = new Set<Listener>();

function emit() {
  listeners.forEach((l) => l());
}

function clone<T>(v: T): T {
  return typeof structuredClone === "function"
    ? structuredClone(v)
    : (JSON.parse(JSON.stringify(v)) as T);
}

async function load() {
  if (loadStarted) return;
  loadStarted = true;
  try {
    current = await getSettings();
    emit();
  } catch (e) {
    loadStarted = false;
    toast(`Couldn't load settings: ${String(e)}`, "error");
  }
  try {
    await onEvent("settings-changed", (s) => {
      // The backend is authoritative unless we have unsaved local edits.
      if (!dirty && !inflight) {
        current = s;
        emit();
      }
    });
  } catch {
    /* events unavailable outside tauri */
  }
}

async function flush() {
  if (!current || !dirty) return;
  if (inflight) return; // the running write will re-check `dirty` when it finishes
  dirty = false;
  const snapshot = clone(current);
  inflight = saveSettings(snapshot)
    .then((normalized) => {
      if (!dirty) {
        current = normalized;
        emit();
      }
    })
    .catch((e) => {
      toast(`Couldn't save settings: ${String(e)}`, "error");
    })
    .finally(() => {
      inflight = null;
      if (dirty) scheduleFlush();
    });
}

function scheduleFlush(delay = 250) {
  window.clearTimeout(flushTimer);
  flushTimer = window.setTimeout(() => void flush(), delay);
}

/** Mutate a draft of the settings; the change is applied immediately and saved shortly after. */
export function updateSettings(mutator: (draft: Settings) => void) {
  if (!current) return;
  const draft = clone(current);
  mutator(draft);
  current = draft;
  dirty = true;
  emit();
  scheduleFlush();
}

export function useSettings() {
  const settings = useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      void load();
      return () => listeners.delete(cb);
    },
    () => current,
  );
  const update = useCallback((mutator: (draft: Settings) => void) => updateSettings(mutator), []);
  return { settings, update, ready: settings !== null };
}

/** Subscribe to a backend event for the lifetime of the component. */
export function useEvent<K extends keyof EventMap>(
  name: K,
  handler: (payload: EventMap[K]) => void,
) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    onEvent(name, (p) => ref.current(p))
      .then((u) => {
        if (cancelled) u();
        else unlisten = u;
      })
      .catch(() => {
        /* not running inside tauri */
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [name]);
}

/** Debounce any value. */
export function useDebounced<T>(value: T, delay = 200): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = window.setTimeout(() => setV(value), delay);
    return () => window.clearTimeout(t);
  }, [value, delay]);
  return v;
}
