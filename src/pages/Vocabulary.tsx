import { useMemo, useState } from "react";
import { BookA, Plus, Search, Trash2 } from "lucide-react";
import { Button } from "../components/Button";
import { ChipList } from "../components/ChipList";
import { EmptyState } from "../components/EmptyState";
import { Group, Hint, LoadingPage, PageHead, Row, Section } from "../components/Field";
import { SegmentedControl } from "../components/SegmentedControl";
import { Toggle } from "../components/Toggle";
import { useSettings } from "../lib/store";
import type { VocabBoost, VocabEntry } from "../lib/types";

const BOOST_HELP: Record<VocabBoost, string> = {
  off: "Prompt and rewrite only.",
  gentle: "A small nudge. Safest.",
  standard: "Recommended.",
  strong: "For words that keep losing to a common look-alike.",
};

export function VocabularyPage() {
  const { settings, update } = useSettings();
  const [query, setQuery] = useState("");
  const [word, setWord] = useState("");
  const [sounds, setSounds] = useState("");

  const entries = settings?.vocabulary ?? [];
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter(
      (e) => e.word.toLowerCase().includes(q) || e.pronunciations.some((p) => p.toLowerCase().includes(q)),
    );
  }, [entries, query]);

  if (!settings) return <LoadingPage />;

  const canAdd = word.trim().length > 0 && !entries.some((e) => e.word.toLowerCase() === word.trim().toLowerCase());

  const add = () => {
    if (!canAdd) return;
    const entry: VocabEntry = {
      id: crypto.randomUUID(),
      word: word.trim(),
      pronunciations: sounds
        .split(",")
        .map((s) => s.trim().toLowerCase())
        .filter((s) => s.length > 0),
      enabled: true,
    };
    update((s) => {
      s.vocabulary.unshift(entry);
    });
    setWord("");
    setSounds("");
  };

  const patch = (id: string, fn: (e: VocabEntry) => void) =>
    update((s) => {
      const e = s.vocabulary.find((x) => x.id === id);
      if (e) fn(e);
    });

  return (
    <div className="page">
      <PageHead
        title="Vocabulary"
        subtitle="Names, products, jargon. Tell Mynah how a word is spelled and how it tends to be misheard."
      />

      <Section title="Add a word">
        <Group>
          <div className="vocab-add">
            <input
              className="input"
              placeholder="Correct spelling, e.g. Kubernetes"
              value={word}
              onChange={(e) => setWord(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && add()}
              aria-label="Word"
            />
            <input
              className="input"
              placeholder="Sounds like (optional), comma separated, e.g. cooper netties, kuber netties"
              value={sounds}
              onChange={(e) => setSounds(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && add()}
              aria-label="Sounds like"
            />
            <Button variant="primary" icon={<Plus size={13} />} disabled={!canAdd} onClick={add}>
              Add
            </Button>
          </div>
        </Group>
        <Hint>
          Every word here is handed to the model as a prompt before it listens and, with the boost below, nudged
          while it decodes. Afterwards, anything in the result that sounds like the word (or like one of its
          “sounds like” spellings) is rewritten to your spelling, so “flicker” becomes “Flickr” even if you never
          typed “flicker”. Add a “sounds like” when the model keeps producing something that sounds different, and
          type the word exactly as you want it to appear, including capitalisation.
        </Hint>
      </Section>

      <Section title="Decoder boost">
        <Group>
          <Row label="Push the model toward your words" description={BOOST_HELP[settings.model.vocabulary_boost]}>
            <SegmentedControl<VocabBoost>
              value={settings.model.vocabulary_boost}
              onChange={(v) => update((s) => void (s.model.vocabulary_boost = v))}
              segments={[
                { value: "off", label: "Off" },
                { value: "gentle", label: "Gentle" },
                { value: "standard", label: "Standard" },
                { value: "strong", label: "Strong" },
              ]}
            />
          </Row>
        </Group>
        <Hint>
          The boost only breaks near-ties: it never adds a word the audio does not support, and speech without your
          words decodes exactly as before. Turn it up if a word still loses to a common look-alike, down if your words
          start appearing where you did not say them.
        </Hint>
      </Section>

      <Section
        title={entries.length > 0 ? `${entries.length} ${entries.length === 1 ? "word" : "words"}` : "Your words"}
        action={
          entries.length > 3 ? (
            <label className="search">
              <Search size={14} />
              <input placeholder="Find a word" value={query} onChange={(e) => setQuery(e.target.value)} />
            </label>
          ) : undefined
        }
      >
        <Group>
          {entries.length === 0 ? (
            <EmptyState
              icon={<BookA size={22} />}
              title="No custom words yet"
              body="Add the names and terms the model keeps getting wrong. They take effect on the next dictation."
            />
          ) : filtered.length === 0 ? (
            <EmptyState title={`Nothing matches “${query}”`} />
          ) : (
            <>
              <div className="vocab-head">
                <span>Spelling</span>
                <span>Sounds like</span>
                <span>On</span>
                <span />
              </div>
              {filtered.map((e) => (
                <div key={e.id} className="vocab">
                  <span className={`vocab-word ${e.enabled ? "" : "off"}`} title={e.word}>
                    {e.word}
                  </span>
                  <ChipList
                    items={e.pronunciations}
                    placeholder="Add"
                    normalize={(s) => s.trim().toLowerCase()}
                    emptyText="Prompt only"
                    onChange={(items) => patch(e.id, (x) => void (x.pronunciations = items))}
                  />
                  <Toggle checked={e.enabled} label={`Enable ${e.word}`} onChange={(v) => patch(e.id, (x) => void (x.enabled = v))} />
                  <Button
                    size="sm"
                    variant="ghost"
                    className="btn-icon"
                    aria-label={`Delete ${e.word}`}
                    onClick={() => update((s) => void (s.vocabulary = s.vocabulary.filter((x) => x.id !== e.id)))}
                  >
                    <Trash2 size={13} />
                  </Button>
                </div>
              ))}
            </>
          )}
        </Group>
      </Section>
    </div>
  );
}
