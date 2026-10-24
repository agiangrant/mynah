import { useEffect, useRef, useState } from "react";
import { disable, enable } from "@tauri-apps/plugin-autostart";
import { Check, CircleDashed, Mic, ShieldCheck } from "lucide-react";
import { Button } from "../components/Button";
import { Group, Hint, LoadingPage, PageHead, Row, Section } from "../components/Field";
import { LevelMeter } from "../components/LevelMeter";
import { SegmentedControl } from "../components/SegmentedControl";
import { Select } from "../components/Select";
import { toast } from "../components/Toast";
import { Toggle } from "../components/Toggle";
import {
  checkPermissions,
  listAudioDevices,
  openPermissionSettings,
  requestPermission,
  startDictation,
  stopDictation,
} from "../lib/ipc";
import { durationShort } from "../lib/format";
import { usePlatform } from "../lib/platform";
import { useEvent, useSettings } from "../lib/store";
import type {
  AudioDevice,
  DictationState,
  InsertMethod,
  PermissionKind,
  Permissions,
  Theme,
} from "../lib/types";

const INSERT_HELP: Record<InsertMethod, string> = {
  auto: "Types short phrases, pastes longer ones. Best for most apps.",
  type: "Simulates keystrokes. Works everywhere, slower for long text.",
  paste: "Puts the text on the clipboard and pastes it, then restores your clipboard.",
};

export function GeneralPage() {
  const { settings, update } = useSettings();
  const platform = usePlatform();
  const [devices, setDevices] = useState<AudioDevice[]>([]);

  useEffect(() => {
    listAudioDevices()
      .then((d) => setDevices(d.filter((x) => x.kind === "input")))
      .catch(() => setDevices([]));
  }, []);

  if (!settings) return <LoadingPage />;
  const g = settings.general;

  const setLaunchAtLogin = async (on: boolean) => {
    update((s) => {
      s.general.launch_at_login = on;
    });
    try {
      if (on) await enable();
      else await disable();
    } catch (e) {
      toast(`Couldn't change login item: ${String(e)}`, "error");
    }
  };

  const deviceOptions = [
    { value: "", label: "System default" },
    ...devices.map((d) => ({ value: d.name, label: d.is_default ? `${d.name} (default)` : d.name })),
  ];
  const deviceValue = g.input_device && devices.some((d) => d.name === g.input_device) ? g.input_device : "";

  return (
    <div className="page">
      <PageHead title="General" subtitle="How Mynah starts, listens, and puts words on the screen." />

      <Section title="Startup">
        <Group>
          <Row label="Open at login" description="Mynah stays in the menu bar, ready for your shortcut.">
            <Toggle checked={g.launch_at_login} onChange={(v) => void setLaunchAtLogin(v)} />
          </Row>
          <Row label="Start hidden" description="Skip this window at launch. Open it from the tray icon.">
            <Toggle checked={g.start_minimized} onChange={(v) => update((s) => void (s.general.start_minimized = v))} />
          </Row>
        </Group>
      </Section>

      <Section title="Feedback">
        <Group>
          <Row label="Show the listening pill" description="A small floating indicator while you dictate.">
            <Toggle checked={g.show_overlay} onChange={(v) => update((s) => void (s.general.show_overlay = v))} />
          </Row>
          <Row label="Sounds" description="Soft clicks when listening starts and stops.">
            <Toggle checked={g.play_sounds} onChange={(v) => update((s) => void (s.general.play_sounds = v))} />
          </Row>
        </Group>
      </Section>

      <Section title="Inserting text">
        <Group>
          <Row label="Insert method" description={INSERT_HELP[g.insert_method]}>
            <Select<InsertMethod>
              value={g.insert_method}
              onChange={(v) => update((s) => void (s.general.insert_method = v))}
              options={[
                { value: "auto", label: "Automatic" },
                { value: "type", label: "Type it" },
                { value: "paste", label: "Paste it" },
              ]}
            />
          </Row>
          <Row label="Add a trailing space" description="So the next thing you type or dictate doesn't run into the last word.">
            <Toggle checked={g.trailing_space} onChange={(v) => update((s) => void (s.general.trailing_space = v))} />
          </Row>
        </Group>
      </Section>

      <Section title="Microphone">
        <Group>
          <Row label="Input device" description="Which microphone Mynah listens to.">
            <Select
              value={deviceValue}
              width={240}
              onChange={(v) => update((s) => void (s.general.input_device = v === "" ? null : v))}
              options={deviceOptions}
            />
          </Row>
        </Group>
        <TestPad />
      </Section>

      {platform === "macos" ? <PermissionsCard /> : null}

      <Section title="Appearance">
        <Group>
          <Row label="Theme">
            <SegmentedControl<Theme>
              value={g.theme}
              onChange={(v) => update((s) => void (s.general.theme = v))}
              segments={[
                { value: "system", label: "System" },
                { value: "light", label: "Light" },
                { value: "dark", label: "Dark" },
              ]}
            />
          </Row>
        </Group>
      </Section>
    </div>
  );
}

// ---------------------------------------------------------------------------

function TestPad() {
  const levelRef = useRef(0);
  const [holding, setHolding] = useState(false);
  const [state, setState] = useState<DictationState | null>(null);
  const holdingRef = useRef(false);

  useEvent("audio-level", (l) => {
    levelRef.current = l.rms;
  });
  useEvent("dictation-state", (d) => {
    if (d.preview || d.phase === "listening" || d.phase === "transcribing") setState(d);
    if (d.phase === "idle") levelRef.current = 0;
  });

  const begin = async () => {
    if (holdingRef.current) return;
    holdingRef.current = true;
    setHolding(true);
    try {
      await startDictation(false);
    } catch (e) {
      holdingRef.current = false;
      setHolding(false);
      toast(`Couldn't start listening: ${String(e)}`, "error");
    }
  };
  const end = async () => {
    if (!holdingRef.current) return;
    holdingRef.current = false;
    setHolding(false);
    levelRef.current = 0;
    try {
      await stopDictation();
    } catch (e) {
      toast(`Couldn't stop: ${String(e)}`, "error");
    }
  };

  // release if the pointer leaves the window while held
  useEffect(() => {
    const up = () => void end();
    window.addEventListener("pointerup", up);
    window.addEventListener("blur", up);
    return () => {
      window.removeEventListener("pointerup", up);
      window.removeEventListener("blur", up);
    };
  }, []);

  const busy = state?.phase === "transcribing";
  const resultText =
    state?.phase === "inserted" && state.text
      ? state.text
      : state?.phase === "error"
        ? state.message ?? "Something went wrong."
        : state?.phase === "cancelled"
          ? "Cancelled."
          : null;

  return (
    <Group className="testpad-group">
      <div className="testpad">
        <div className="testpad-top">
          <button
            type="button"
            className={`hold-btn ${holding ? "holding" : ""} ${busy ? "busy" : ""}`}
            onPointerDown={(e) => {
              e.preventDefault();
              void begin();
            }}
            onPointerUp={() => void end()}
            onPointerCancel={() => void end()}
            onKeyDown={(e) => {
              if ((e.key === " " || e.key === "Enter") && !e.repeat) void begin();
            }}
            onKeyUp={(e) => {
              if (e.key === " " || e.key === "Enter") void end();
            }}
            aria-pressed={holding}
          >
            <Mic size={15} />
            {holding ? "Listening…" : busy ? "Transcribing…" : "Hold to test"}
          </button>
          <LevelMeter levelRef={levelRef} active={holding} bars={9} height={26} />
          <span className="faint" style={{ marginLeft: "auto", fontSize: 12 }}>
            Nothing is typed anywhere.
          </span>
        </div>
        <div className={`testpad-result ${resultText ? "" : "placeholder"}`}>
          {resultText ??
            (busy ? "Working on it…" : "Hold the button, say a sentence, let go. The result shows up here.")}
        </div>
        {state?.phase === "inserted" && (state.latency_ms !== null || state.duration_ms !== null) ? (
          <div className="testpad-stats">
            {state.duration_ms !== null ? <span>Recording {durationShort(state.duration_ms)}</span> : null}
            {state.latency_ms !== null ? <span>Transcribed in {durationShort(state.latency_ms)}</span> : null}
          </div>
        ) : null}
      </div>
    </Group>
  );
}

// ---------------------------------------------------------------------------

const PERMS: { kind: PermissionKind; label: string; why: string }[] = [
  { kind: "microphone", label: "Microphone", why: "Needed to hear you." },
  { kind: "input_monitoring", label: "Input monitoring", why: "Lets the hold-to-dictate key work in any app." },
  { kind: "accessibility", label: "Accessibility", why: "Lets Mynah type the result where your cursor is." },
];

function PermissionsCard() {
  const [perms, setPerms] = useState<Permissions | null>(null);

  useEffect(() => {
    let alive = true;
    const poll = () =>
      checkPermissions()
        .then((p) => alive && setPerms(p))
        .catch(() => undefined);
    void poll();
    const t = window.setInterval(() => void poll(), 2000);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, []);

  const grant = async (kind: PermissionKind) => {
    try {
      await requestPermission(kind);
      await openPermissionSettings(kind);
    } catch (e) {
      toast(`Couldn't open System Settings: ${String(e)}`, "error");
    }
  };

  const allGood = perms && PERMS.every((p) => perms[p.kind] !== false);

  return (
    <Section
      title="Permissions"
      description={
        allGood
          ? "Everything Mynah needs is granted."
          : "macOS asks you to allow these once. If a toggle is already on in System Settings and still shows here as missing, turn it off and on again."
      }
    >
      <Group>
        {PERMS.map((p) => {
          const v = perms?.[p.kind];
          return (
            <Row key={p.kind} label={p.label} description={p.why}>
              {v === true ? (
                <span className="perm-state">
                  <Check size={14} color="var(--ok)" /> Granted
                </span>
              ) : v === false ? (
                <>
                  <span className="perm-state">
                    <CircleDashed size={14} /> Not granted
                  </span>
                  <Button size="sm" variant="primary" onClick={() => void grant(p.kind)}>
                    Grant…
                  </Button>
                </>
              ) : (
                <span className="perm-state faint">
                  <ShieldCheck size={14} /> Checking
                </span>
              )}
            </Row>
          );
        })}
      </Group>
      <Hint>After granting Input monitoring or Accessibility, macOS may require relaunching Mynah.</Hint>
    </Section>
  );
}
