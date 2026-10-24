import { useEffect, useState } from "react";
import { Circle, X } from "lucide-react";
import { cancelShortcutCapture, startShortcutCapture } from "../lib/ipc";
import { useEvent } from "../lib/store";
import { Button } from "./Button";
import { KeyCombo } from "./KeyCap";
import { toast } from "./Toast";

/**
 * Records a global shortcut. Only one recorder is active at a time since the
 * backend has a single capture mode; `shortcut-capture` events feed the live keys.
 */
export function ShortcutRecorder({
  value,
  onChange,
  clearable = false,
}: {
  value: string[] | null;
  onChange: (keys: string[] | null) => void;
  clearable?: boolean;
}) {
  const [recording, setRecording] = useState(false);
  const [live, setLive] = useState<string[]>([]);

  useEvent("shortcut-capture", (c) => {
    if (!recording) return;
    setLive(c.keys);
    if (c.done) {
      setRecording(false);
      if (c.keys.length > 0) onChange(c.keys);
      setLive([]);
    }
  });

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void stop();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording]);

  const start = async () => {
    try {
      setLive([]);
      setRecording(true);
      await startShortcutCapture();
    } catch (e) {
      setRecording(false);
      toast(`Couldn't start recording: ${String(e)}`, "error");
    }
  };

  const stop = async () => {
    setRecording(false);
    setLive([]);
    try {
      await cancelShortcutCapture();
    } catch {
      /* ignore */
    }
  };

  return (
    <div className={`recorder ${recording ? "recording" : ""}`}>
      <div className="recorder-keys">
        {recording ? (
          live.length > 0 ? (
            <KeyCombo keys={live} size="lg" />
          ) : (
            <span className="recorder-prompt">
              <Circle size={8} className="rec-dot" /> Press your key or combination
            </span>
          )
        ) : (
          <KeyCombo keys={value ?? []} size="lg" placeholder={clearable ? "None" : "Not set"} />
        )}
      </div>
      <div className="recorder-actions">
        {recording ? (
          <Button size="sm" variant="ghost" onClick={() => void stop()}>
            Cancel
          </Button>
        ) : (
          <>
            {clearable && value && value.length > 0 ? (
              <Button size="sm" variant="ghost" icon={<X size={13} />} onClick={() => onChange(null)}>
                Clear
              </Button>
            ) : null}
            <Button size="sm" onClick={() => void start()}>
              {value && value.length > 0 ? "Change" : "Record"}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
