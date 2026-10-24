import { useSyncExternalStore } from "react";
import { getSystemInfo } from "./ipc";
import type { SystemInfo } from "./types";

export type Platform = SystemInfo["platform"];

function guess(): Platform {
  const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
  if (/Mac|iPhone|iPad/.test(ua)) return "macos";
  if (/Win/.test(ua)) return "windows";
  return "linux";
}

let info: SystemInfo | null = null;
let platform: Platform = guess();
let loading: Promise<void> | null = null;
const listeners = new Set<() => void>();

function emit() {
  listeners.forEach((l) => l());
}

export function loadSystemInfo(): Promise<void> {
  if (!loading) {
    loading = getSystemInfo()
      .then((i) => {
        info = i;
        platform = i.platform;
        emit();
      })
      .catch(() => {
        /* keep the guess */
      });
  }
  return loading;
}

export function usePlatform(): Platform {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      void loadSystemInfo();
      return () => listeners.delete(cb);
    },
    () => platform,
  );
}

export function useSystemInfo(): SystemInfo | null {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      void loadSystemInfo();
      return () => listeners.delete(cb);
    },
    () => info,
  );
}
