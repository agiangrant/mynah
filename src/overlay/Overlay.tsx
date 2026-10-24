import { useEffect, useRef, useState } from "react";
import { AlertCircle, Check } from "lucide-react";
import { dismissMeetingPrompt, getDictationState, getMeetingState, startMeeting } from "../lib/ipc";
import { mmss } from "../lib/format";
import { keyLabel, sortKeys } from "../lib/keys";
import { usePlatform } from "../lib/platform";
import { useEvent, useSettings } from "../lib/store";
import type { DictationState, MeetingState } from "../lib/types";

const IDLE: DictationState = {
  phase: "idle",
  text: null,
  message: null,
  duration_ms: null,
  latency_ms: null,
  preview: false,
  progress: null,
};
const NO_MEETING: MeetingState = {
  active: false,
  app: null,
  started_at: null,
  segments: [],
  entry_id: null,
  pending_app: null,
};

export function Overlay() {
  const [dict, setDict] = useState<DictationState>(IDLE);
  const [meeting, setMeeting] = useState<MeetingState>(NO_MEETING);
  const level = useRef(0);
  const { settings } = useSettings();
  const platform = usePlatform();

  useEffect(() => {
    getDictationState().then(setDict).catch(() => undefined);
    getMeetingState().then(setMeeting).catch(() => undefined);
  }, []);
  useEvent("dictation-state", (d) => {
    setDict(d);
    if (d.phase !== "listening") level.current = 0;
  });
  useEvent("audio-level", (l) => {
    level.current = l.rms;
  });
  useEvent("meeting-state", setMeeting);
  useEvent("meeting-segment", (seg) =>
    setMeeting((m) => (m.active ? { ...m, segments: [...m.segments, seg] } : m)),
  );

  const keys = settings?.shortcuts.dictation.keys ?? [];
  const keyHint = sortKeys(keys)
    .map((k) => keyLabel(k, platform).main)
    .join(platform === "macos" ? "" : "+");

  let content: React.ReactNode = null;

  if (dict.phase === "listening") {
    content = (
      <div className="pill">
        <Bars levelRef={level} />
        <span className="pill-label">Listening</span>
        {keyHint ? <span className="kbd">{keyHint}</span> : null}
      </div>
    );
  } else if (dict.phase === "transcribing") {
    content = (
      <div className="pill">
        <span className="spinner" />
        <span className="pill-label">
          Transcribing…{dict.progress != null && dict.progress > 0 ? ` ${dict.progress}%` : ""}
        </span>
      </div>
    );
  } else if (dict.phase === "inserted") {
    content = (
      <div className="pill">
        <span className="check">
          <Check size={16} />
        </span>
        <span className="pill-text">{dict.text && dict.text.trim() ? dict.text.trim() : "Nothing heard"}</span>
      </div>
    );
  } else if (dict.phase === "error") {
    content = (
      <div className="pill">
        <span className="err">
          <AlertCircle size={16} />
        </span>
        <span className="pill-text">{dict.message ?? "Something went wrong"}</span>
      </div>
    );
  } else if (dict.phase === "cancelled") {
    content = (
      <div className="pill">
        <span className="pill-muted">Cancelled</span>
      </div>
    );
  } else if (meeting.pending_app) {
    content = (
      <div className="pill prompt">
        <span className="pill-text">
          <span className="pill-label">{meeting.pending_app}</span>
          <span className="pill-muted"> detected · Transcribe this meeting?</span>
        </span>
        <button className="pbtn" onClick={() => void dismissMeetingPrompt().catch(() => undefined)}>
          Dismiss
        </button>
        <button className="pbtn primary" onClick={() => void startMeeting(meeting.pending_app).catch(() => undefined)}>
          Start
        </button>
      </div>
    );
  } else if (meeting.active) {
    content = <MeetingPill meeting={meeting} />;
  }

  return <div className="pill-wrap">{content}</div>;
}

function MeetingPill({ meeting }: { meeting: MeetingState }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, []);
  const elapsed = meeting.started_at ? Math.max(0, now - Date.parse(meeting.started_at)) : 0;
  return (
    <div className="pill">
      <span className="rec" />
      <span className="pill-label">Recording meeting</span>
      <span className="pill-muted time">{mmss(elapsed)}</span>
    </div>
  );
}

function Bars({ levelRef, count = 7 }: { levelRef: React.MutableRefObject<number>; count?: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const cur = useRef<number[]>(Array.from({ length: count }, () => 0.12));
  useEffect(() => {
    let raf = 0;
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    const weights = Array.from({ length: count }, (_, i) => {
      const c = (count - 1) / 2;
      return 1 - (Math.abs(i - c) / Math.max(c, 1)) * 0.5;
    });
    const tick = () => {
      const el = ref.current;
      if (el) {
        const target = Math.min(1, Math.pow(levelRef.current * 4, 0.7));
        const t = performance.now() / 1000;
        for (let i = 0; i < count; i++) {
          const jitter = 0.1 * Math.sin(t * (5 + i * 1.3) + i);
          const goal = Math.max(0.12, (target + jitter) * weights[i]);
          const c = cur.current[i];
          cur.current[i] = reduce ? goal : c + (goal - c) * (goal > c ? 0.5 : 0.2);
          const bar = el.children[i] as HTMLElement | undefined;
          if (bar) bar.style.transform = `scaleY(${cur.current[i].toFixed(3)})`;
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [count, levelRef]);
  return (
    <div className="bars" ref={ref} aria-hidden>
      {Array.from({ length: count }, (_, i) => (
        <span key={i} className="bar" />
      ))}
    </div>
  );
}
