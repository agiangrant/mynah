import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Square, Users } from "lucide-react";
import { Button } from "../components/Button";
import { ChipList } from "../components/ChipList";
import { Group, Hint, LoadingPage, PageHead, Row, Section } from "../components/Field";
import { Select } from "../components/Select";
import { SegmentedControl } from "../components/SegmentedControl";
import { toast } from "../components/Toast";
import { Toggle } from "../components/Toggle";
import { dismissMeetingPrompt, getMeetingState, listAudioDevices, startMeeting, stopMeeting } from "../lib/ipc";
import { mmss } from "../lib/format";
import { usePlatform } from "../lib/platform";
import { useEvent, useSettings } from "../lib/store";
import type { AudioDevice, MeetingSegment, MeetingState, SaveFormat } from "../lib/types";

const EMPTY: MeetingState = {
  active: false,
  app: null,
  started_at: null,
  segments: [],
  entry_id: null,
  pending_app: null,
};

const IDLE_OPTIONS = [
  { value: "30", label: "30 seconds" },
  { value: "60", label: "1 minute" },
  { value: "120", label: "2 minutes" },
  { value: "300", label: "5 minutes" },
];

export function MeetingsPage() {
  const { settings, update } = useSettings();
  const platform = usePlatform();
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [state, setState] = useState<MeetingState>(EMPTY);

  useEffect(() => {
    listAudioDevices().then(setDevices).catch(() => setDevices([]));
    getMeetingState().then(setState).catch(() => undefined);
  }, []);
  useEvent("meeting-state", setState);
  useEvent("meeting-segment", (seg) =>
    setState((s) => (s.active ? { ...s, segments: [...s.segments, seg] } : s)),
  );

  if (!settings) return <LoadingPage />;
  const m = settings.meetings;

  const act = (fn: () => Promise<unknown>) => fn().catch((e) => toast(String(e), "error"));

  const chooseDir = async () => {
    try {
      const picked = await open({ directory: true, multiple: false });
      if (!picked) return;
      const path = typeof picked === "string" ? picked : String(picked);
      update((s) => void (s.meetings.save_dir = path));
    } catch (e) {
      toast(`Couldn't choose a folder: ${String(e)}`, "error");
    }
  };

  const sysHint =
    platform === "windows"
      ? "On Windows pick your speakers or headphones; Mynah records what they play (loopback)."
      : platform === "macos"
        ? "On macOS pick a virtual device such as BlackHole or an aggregate device that carries your system audio."
        : "On Linux pick the monitor source of your output device (PulseAudio or PipeWire).";

  const deviceOptions = [
    { value: "", label: "Not set" },
    ...devices.map((d) => ({
      value: d.name,
      label: `${d.name}${d.kind === "output" ? " (output)" : ""}`,
    })),
  ];
  const sysValue = m.system_audio_device && devices.some((d) => d.name === m.system_audio_device) ? m.system_audio_device : "";
  const idleValue = IDLE_OPTIONS.some((o) => o.value === String(m.end_after_idle_secs)) ? String(m.end_after_idle_secs) : "60";

  return (
    <div className="page">
      <PageHead title="Meetings" subtitle="Notice when a call starts and keep a transcript, without sending audio anywhere." />

      <LiveCard state={state} onStart={() => act(() => startMeeting(state.pending_app))} onStop={() => act(stopMeeting)} onDismiss={() => act(dismissMeetingPrompt)} />

      <Section title="Detection">
        <Group>
          <Row label="Detect meetings" description="Watch for meeting apps and offer to transcribe.">
            <Toggle checked={m.detection_enabled} onChange={(v) => update((s) => void (s.meetings.detection_enabled = v))} />
          </Row>
          <Row label="When a meeting starts" disabled={!m.detection_enabled}>
            <SegmentedControl
              value={m.auto_start ? "auto" : "ask"}
              onChange={(v) => update((s) => void (s.meetings.auto_start = v === "auto"))}
              segments={[
                { value: "ask", label: "Ask first" },
                { value: "auto", label: "Start transcribing" },
              ]}
            />
          </Row>
          <Row label="Only when the microphone is in use" description="Chat apps like Slack and Discord are always open; this waits for an actual call." disabled={!m.detection_enabled}>
            <Toggle checked={m.require_mic_in_use} onChange={(v) => update((s) => void (s.meetings.require_mic_in_use = v))} />
          </Row>
          <Row stack label="Meeting apps" description="Process names, matched loosely. Add anything you use." disabled={!m.detection_enabled}>
            <ChipList items={m.apps} placeholder="Add an app" onChange={(items) => update((s) => void (s.meetings.apps = items))} />
          </Row>
          <Row label="Stop after silence" description="Once the app closes or the mic goes quiet for this long, the transcript is saved." disabled={!m.detection_enabled}>
            <Select value={idleValue} width={150} options={IDLE_OPTIONS} onChange={(v) => update((s) => void (s.meetings.end_after_idle_secs = Number(v)))} />
          </Row>
        </Group>
      </Section>

      <Section title="Audio">
        <Group>
          <Row label="Also capture the other participants" description="Your microphone is always transcribed as “You”. This adds what you hear as “Others”.">
            <Toggle checked={m.include_system_audio} onChange={(v) => update((s) => void (s.meetings.include_system_audio = v))} />
          </Row>
          <Row label="System audio device" description={sysHint} disabled={!m.include_system_audio}>
            <Select value={sysValue} width={240} options={deviceOptions} onChange={(v) => update((s) => void (s.meetings.system_audio_device = v === "" ? null : v))} />
          </Row>
        </Group>
      </Section>

      <Section title="Transcripts">
        <Group>
          <Row label="Save to" description={m.save_dir ? <span className="path" title={m.save_dir}>{m.save_dir}</span> : "The Mynah data folder."}>
            {m.save_dir ? (
              <Button size="sm" variant="ghost" onClick={() => update((s) => void (s.meetings.save_dir = null))}>
                Use default
              </Button>
            ) : null}
            <Button size="sm" icon={<FolderOpen size={13} />} onClick={() => void chooseDir()}>
              Choose…
            </Button>
          </Row>
          <Row label="Format">
            <SegmentedControl<SaveFormat>
              value={m.save_format}
              onChange={(v) => update((s) => void (s.meetings.save_format = v))}
              segments={[
                { value: "md", label: "Markdown" },
                { value: "txt", label: "Plain text" },
              ]}
            />
          </Row>
        </Group>
        <Hint>Transcripts also appear in History, where you can search and copy them.</Hint>
      </Section>
    </div>
  );
}

// ---------------------------------------------------------------------------

function LiveCard({
  state,
  onStart,
  onStop,
  onDismiss,
}: {
  state: MeetingState;
  onStart: () => void;
  onStop: () => void;
  onDismiss: () => void;
}) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!state.active) return;
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [state.active]);

  const elapsed = state.started_at ? Math.max(0, now - Date.parse(state.started_at)) : 0;

  return (
    <Section title="Now">
      {state.pending_app ? (
        <div className="prompt">
          <Users size={16} />
          <span className="prompt-text">
            <strong>{state.pending_app}</strong> looks like a meeting. Transcribe it?
          </span>
          <Button size="sm" variant="ghost" onClick={onDismiss}>
            Not now
          </Button>
          <Button size="sm" variant="primary" onClick={onStart}>
            Transcribe
          </Button>
        </div>
      ) : null}
      <Group>
        <div className="live">
          {state.active ? (
            <>
              <div className="live-head">
                <span className="live-dot" />
                <div>
                  <div className="live-title">Transcribing {state.app ?? "meeting"}</div>
                  <div className="live-sub">
                    {mmss(elapsed)} · {state.segments.length} {state.segments.length === 1 ? "segment" : "segments"}
                  </div>
                </div>
                <div className="live-actions">
                  <Button size="sm" variant="danger" icon={<Square size={12} />} onClick={onStop}>
                    Stop and save
                  </Button>
                </div>
              </div>
              <Transcript segments={state.segments} follow />
            </>
          ) : (
            <div className="live-head">
              <div>
                <div className="live-title">Not transcribing</div>
                <div className="live-sub">Start by hand any time, with or without a meeting app.</div>
              </div>
              <div className="live-actions">
                <Button size="sm" variant="primary" onClick={onStart}>
                  Start transcribing
                </Button>
              </div>
            </div>
          )}
        </div>
      </Group>
    </Section>
  );
}

export function Transcript({ segments, follow = false }: { segments: MeetingSegment[]; follow?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (follow && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [segments.length, follow]);
  return (
    <div className="transcript" ref={ref}>
      {segments.length === 0 ? (
        <div className="transcript-empty">Listening. Text appears here a few seconds after someone speaks.</div>
      ) : (
        segments.map((s, i) => (
          <div key={`${s.start_ms}-${i}`} className="seg">
            <span className="seg-time">{mmss(s.start_ms)}</span>
            <span className={`seg-who ${s.source === "mic" ? "me" : ""}`}>{s.source === "mic" ? "You" : "Others"}</span>
            <span>{s.text}</span>
          </div>
        ))
      )}
    </div>
  );
}
