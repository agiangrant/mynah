import { Group, Hint, LoadingPage, PageHead, Row, Section } from "../components/Field";
import { KeyCap } from "../components/KeyCap";
import { SegmentedControl } from "../components/SegmentedControl";
import { ShortcutRecorder } from "../components/ShortcutRecorder";
import { usePlatform } from "../lib/platform";
import { useSettings } from "../lib/store";
import type { DictationMode } from "../lib/types";

const MODE_HELP: Record<DictationMode, string> = {
  hold: "Speak while the key is held. Release to insert.",
  toggle: "Press once to start, press again to stop and insert.",
  auto: "Tap to toggle, hold to push-to-talk. Mynah tells them apart by how long you press.",
};

export function ShortcutsPage() {
  const { settings, update } = useSettings();
  const platform = usePlatform();
  if (!settings) return <LoadingPage />;
  const sc = settings.shortcuts;

  const tips =
    platform === "macos"
      ? ["MetaRight", "AltGr", "Function"]
      : platform === "windows"
        ? ["ControlRight", "AltGr"]
        : ["ControlRight", "MetaRight"];

  return (
    <div className="page">
      <PageHead title="Shortcuts" subtitle="Global keys that work in every app. A single modifier on its own works too." />

      <Section title="Dictate">
        <Group>
          <Row stack label="Dictation key" description={MODE_HELP[sc.dictation_mode]}>
            <ShortcutRecorder
              value={sc.dictation.keys}
              onChange={(keys) => update((s) => void (s.shortcuts.dictation = { keys: keys ?? [] }))}
            />
          </Row>
          <Row label="Behaviour">
            <SegmentedControl<DictationMode>
              value={sc.dictation_mode}
              onChange={(v) => update((s) => void (s.shortcuts.dictation_mode = v))}
              segments={[
                { value: "hold", label: "Hold" },
                { value: "toggle", label: "Toggle" },
                { value: "auto", label: "Auto" },
              ]}
            />
          </Row>
        </Group>
        <Hint>
          Good choices that don't collide with anything:{" "}
          {tips.map((k, i) => (
            <span key={k}>
              {i > 0 ? ", " : ""}
              <KeyCap name={k} size="sm" />
            </span>
          ))}
          . Combinations like <KeyCap name="ControlLeft" size="sm" /> + <KeyCap name="Space" size="sm" /> work as well.
        </Hint>
      </Section>

      <Section title="Meetings">
        <Group>
          <Row stack label="Start or stop meeting transcription" description="Optional. Handy when detection is off or a meeting app isn't recognised.">
            <ShortcutRecorder
              clearable
              value={sc.meeting_toggle?.keys ?? null}
              onChange={(keys) => update((s) => void (s.shortcuts.meeting_toggle = keys ? { keys } : null))}
            />
          </Row>
        </Group>
      </Section>

      <Section title="Cancel">
        <Group>
          <Row stack label="Discard the current dictation" description="Press while listening to throw the recording away without inserting anything.">
            <ShortcutRecorder
              clearable
              value={sc.cancel?.keys ?? null}
              onChange={(keys) => update((s) => void (s.shortcuts.cancel = keys ? { keys } : null))}
            />
          </Row>
        </Group>
      </Section>
    </div>
  );
}
