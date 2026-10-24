import { Button } from "../components/Button";
import { ChipList } from "../components/ChipList";
import { Group, Hint, LoadingPage, PageHead, Row, Section } from "../components/Field";
import { Select } from "../components/Select";
import { Toggle } from "../components/Toggle";
import { useSettings } from "../lib/store";

export const DEFAULT_FILLERS = [
  "uh",
  "um",
  "uhm",
  "umm",
  "hmm",
  "hm",
  "mm",
  "er",
  "erm",
  "ah",
  "eh",
  "mhm",
  "uh-huh",
];

const WARM_OPTIONS = [
  { value: "0", label: "Release right away" },
  { value: "5", label: "5 seconds" },
  { value: "30", label: "30 seconds" },
  { value: "60", label: "1 minute" },
  { value: "300", label: "5 minutes" },
];

export function DictationPage() {
  const { settings, update } = useSettings();
  if (!settings) return <LoadingPage />;
  const d = settings.dictation;

  const warmValue = WARM_OPTIONS.some((o) => o.value === String(d.keep_mic_warm_secs))
    ? String(d.keep_mic_warm_secs)
    : "0";

  return (
    <div className="page">
      <PageHead title="Dictation" subtitle="What happens to your words between the microphone and the cursor." />

      <Section title="Clean-up">
        <Group>
          <Row label="Remove filler words" description="Drops the ums and uhs, then tidies the punctuation around them.">
            <Toggle checked={d.remove_fillers} onChange={(v) => update((s) => void (s.dictation.remove_fillers = v))} />
          </Row>
          <Row
            stack
            label="Words to remove"
            description="Matched as whole words, any capitalisation. Press Enter after each one."
            disabled={!d.remove_fillers}
          >
            <div className="stack">
              <ChipList
                items={d.filler_words}
                normalize={(s) => s.trim().toLowerCase()}
                placeholder="Add a word"
                emptyText="No words yet."
                onChange={(items) => update((s) => void (s.dictation.filler_words = items))}
              />
              <div>
                <Button size="sm" variant="ghost" onClick={() => update((s) => void (s.dictation.filler_words = [...DEFAULT_FILLERS]))}>
                  Reset to defaults
                </Button>
              </div>
            </div>
          </Row>
          <Row label="Capitalise the first word" description="Also fixes the case after sentence breaks.">
            <Toggle checked={d.auto_capitalize} onChange={(v) => update((s) => void (s.dictation.auto_capitalize = v))} />
          </Row>
          <Row
            label="Ignore phantom phrases"
            description="Speech models sometimes invent “Thank you.” or “Subtitles by…” on silence. Drop those."
          >
            <Toggle checked={d.hallucination_filter} onChange={(v) => update((s) => void (s.dictation.hallucination_filter = v))} />
          </Row>
        </Group>
      </Section>

      <Section title="Timing">
        <Group>
          <Row label="Shortest recording" description="Taps shorter than this are ignored, so an accidental press inserts nothing.">
            <span className="hstack">
              <input
                type="range"
                min={150}
                max={1500}
                step={50}
                value={d.min_speech_ms}
                onChange={(e) => update((s) => void (s.dictation.min_speech_ms = Number(e.target.value)))}
                aria-label="Shortest recording in milliseconds"
              />
              <span className="range-value">{d.min_speech_ms} ms</span>
            </span>
          </Row>
          <Row label="Keep the microphone open" description="After you finish, keep the mic ready so the next press starts instantly. The mic indicator stays on meanwhile.">
            <Select
              value={warmValue}
              width={180}
              options={WARM_OPTIONS}
              onChange={(v) => update((s) => void (s.dictation.keep_mic_warm_secs = Number(v)))}
            />
          </Row>
        </Group>
        <Hint>Punctuation comes from the model itself. Say “comma” or “new line” and it is usually written as punctuation.</Hint>
      </Section>
    </div>
  );
}
