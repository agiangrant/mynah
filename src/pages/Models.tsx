import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Check, Cpu, Download, FolderOpen, Loader2, Trash2, X } from "lucide-react";
import { Badge } from "../components/Chip";
import { Button } from "../components/Button";
import { Group, Hint, LoadingPage, PageHead, Row, Section } from "../components/Field";
import { DotMeter, ProgressBar } from "../components/ProgressBar";
import { SegmentedControl } from "../components/SegmentedControl";
import { Select } from "../components/Select";
import { toast } from "../components/Toast";
import { Toggle } from "../components/Toggle";
import {
  cancelDownload,
  deleteModel,
  downloadModel,
  getEngineStatus,
  importCustomModel,
  listModels,
  selectModel,
} from "../lib/ipc";
import { humanSize } from "../lib/format";
import { useEvent, useSettings } from "../lib/store";
import type { Decoding, DownloadProgress, EngineStatus, ModelInfo } from "../lib/types";

const LANGS: { value: string; label: string }[] = [
  { value: "auto", label: "Detect automatically" },
  { value: "en", label: "English" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
  { value: "it", label: "Italian" },
  { value: "pt", label: "Portuguese" },
  { value: "nl", label: "Dutch" },
  { value: "ja", label: "Japanese" },
  { value: "zh", label: "Chinese" },
  { value: "ko", label: "Korean" },
  { value: "ru", label: "Russian" },
  { value: "hi", label: "Hindi" },
  { value: "ar", label: "Arabic" },
  { value: "sv", label: "Swedish" },
  { value: "pl", label: "Polish" },
  { value: "tr", label: "Turkish" },
  { value: "uk", label: "Ukrainian" },
];

const DECODING_HELP: Record<Decoding, string> = {
  fast: "Greedy decoding. Lowest latency.",
  balanced: "Greedy with fallback on uncertain audio. The default.",
  accurate: "Beam search. Best accuracy, a little slower.",
};

const BACKEND_LABEL: Record<string, string> = {
  metal: "Metal",
  cuda: "CUDA",
  vulkan: "Vulkan",
  cpu: "CPU",
};

export function ModelsPage() {
  const { settings, update } = useSettings();
  const [models, setModels] = useState<ModelInfo[] | null>(null);
  const [engine, setEngine] = useState<EngineStatus | null>(null);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const refresh = useCallback(() => {
    listModels()
      .then(setModels)
      .catch((e) => toast(`Couldn't list models: ${String(e)}`, "error"));
  }, []);

  useEffect(() => {
    refresh();
    getEngineStatus().then(setEngine).catch(() => undefined);
  }, [refresh]);

  useEvent("models-changed", refresh);
  useEvent("engine-status", setEngine);
  useEvent("model-download-progress", (p) => {
    setProgress((prev) => {
      const next = { ...prev };
      if (p.done || p.error) delete next[p.id];
      else next[p.id] = p;
      return next;
    });
    if (p.error) toast(`Download failed: ${p.error}`, "error");
    if (p.done || p.error) refresh();
  });

  if (!settings || !models) return <LoadingPage />;
  const m = settings.model;
  const selectedId = m.selected_model_id;

  const act = async (fn: () => Promise<unknown>, after?: () => void) => {
    try {
      await fn();
      after?.();
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const onImport = async () => {
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "GGML model", extensions: ["bin", "gguf"] }],
      });
      if (!picked) return;
      const path = typeof picked === "string" ? picked : (picked as { path?: string }).path ?? String(picked);
      const info = await importCustomModel(path);
      toast(`Imported ${info.name}`, "success");
      refresh();
    } catch (e) {
      toast(`Couldn't import model: ${String(e)}`, "error");
    }
  };

  const groups: { title: string; desc?: string; items: ModelInfo[] }[] = [
    {
      title: "Recommended",
      desc: "Picked for this machine. Start here.",
      items: models.filter((x) => x.recommended && !x.custom),
    },
    {
      title: "English only",
      desc: "Smaller and faster when you only dictate in English.",
      items: models.filter((x) => !x.recommended && !x.multilingual && !x.custom),
    },
    {
      title: "Multilingual",
      desc: "Understand and translate dozens of languages.",
      items: models.filter((x) => !x.recommended && x.multilingual && !x.custom),
    },
    { title: "Custom", desc: "Models you imported yourself.", items: models.filter((x) => x.custom) },
  ].filter((g) => g.items.length > 0 || g.title === "Custom");

  return (
    <div className="page">
      <PageHead title="Models" subtitle="Everything runs on this machine. Bigger models hear better; smaller ones answer faster." />

      <EngineBanner engine={engine} models={models} />

      {groups.map((g) => (
        <Section
          key={g.title}
          title={g.title}
          description={g.desc}
          action={
            g.title === "Custom" ? (
              <Button size="sm" icon={<FolderOpen size={13} />} onClick={() => void onImport()}>
                Import model…
              </Button>
            ) : undefined
          }
        >
          {g.items.length === 0 ? (
            <Group>
              <div className="row">
                <div className="row-main">
                  <div className="row-desc">
                    Any whisper.cpp GGML file works. Import one and it appears here, ready to select.
                  </div>
                </div>
              </div>
            </Group>
          ) : (
            <Group>
              {g.items.map((x) => {
                const p = progress[x.id];
                const downloading = !!p || x.downloading;
                const selected = x.id === selectedId;
                const loadingThis = engine?.state === "loading" && engine.model_id === x.id;
                return (
                  <div key={x.id} className={`model ${selected ? "selected" : ""}`}>
                    <div className="model-head">
                      <span className="model-name">{x.name}</span>
                      {x.recommended ? <Badge tone="accent">Recommended</Badge> : null}
                      {!x.multilingual ? <Badge>English</Badge> : null}
                      {x.quantized ? <Badge>Quantized</Badge> : null}
                      {x.custom ? <Badge>Custom</Badge> : null}
                    </div>
                    <div className="model-desc">{x.description}</div>
                    <div className="model-meta">
                      <span>{humanSize(x.size_bytes)}</span>
                      <DotMeter label="Speed" value={x.speed} />
                      <DotMeter label="Accuracy" value={x.accuracy} />
                    </div>
                    <div className="model-actions">
                      {downloading ? null : x.installed ? (
                        <>
                          <button
                            type="button"
                            className={`select-btn ${selected ? "on" : ""}`}
                            onClick={() => !selected && void act(() => selectModel(x.id))}
                            disabled={loadingThis}
                            aria-pressed={selected}
                          >
                            <span className={`radio ${selected ? "on" : ""}`} />
                            {loadingThis ? "Loading…" : selected ? "Selected" : "Select"}
                          </button>
                          {confirmDelete === x.id ? (
                            <span className="confirm">
                              Delete?
                              <Button size="sm" variant="danger" onClick={() => void act(() => deleteModel(x.id), () => { setConfirmDelete(null); refresh(); })}>
                                Yes
                              </Button>
                              <Button size="sm" variant="ghost" onClick={() => setConfirmDelete(null)}>
                                No
                              </Button>
                            </span>
                          ) : (
                            <Button
                              size="sm"
                              variant="ghost"
                              className="btn-icon"
                              aria-label="Delete model"
                              title="Delete model file"
                              onClick={() => setConfirmDelete(x.id)}
                            >
                              <Trash2 size={13} />
                            </Button>
                          )}
                        </>
                      ) : x.url ? (
                        <Button size="sm" icon={<Download size={13} />} onClick={() => void act(() => downloadModel(x.id), refresh)}>
                          Download
                        </Button>
                      ) : null}
                    </div>
                    {downloading ? (
                      <div className="model-progress">
                        <ProgressBar
                          value={p && p.total ? p.downloaded / p.total : 0}
                          indeterminate={!p || !p.total}
                        />
                        <span>
                          {p ? `${humanSize(p.downloaded)}${p.total ? ` of ${humanSize(p.total)}` : ""}` : "Starting…"}
                        </span>
                        <Button size="sm" variant="ghost" className="btn-icon" aria-label="Cancel download" onClick={() => void act(() => cancelDownload(x.id), refresh)}>
                          <X size={13} />
                        </Button>
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </Group>
          )}
        </Section>
      ))}

      <Section title="Compute" description="How the selected model runs. Changes apply to the next transcription.">
        <Group>
          <Row label="Use the GPU" description="Metal on Apple silicon, CUDA or Vulkan elsewhere. Turn off if you see garbled output.">
            <Toggle checked={m.use_gpu} onChange={(v) => update((s) => void (s.model.use_gpu = v))} />
          </Row>
          <Row label="CPU threads" description="0 picks a sensible number for this machine.">
            <input
              className="input input-num"
              type="number"
              min={0}
              max={64}
              value={m.threads}
              onChange={(e) => {
                const n = Math.max(0, Math.min(64, Math.round(Number(e.target.value) || 0)));
                update((s) => void (s.model.threads = n));
              }}
            />
          </Row>
          <Row label="Decoding" description={DECODING_HELP[m.decoding]}>
            <SegmentedControl<Decoding>
              value={m.decoding}
              onChange={(v) => update((s) => void (s.model.decoding = v))}
              segments={[
                { value: "fast", label: "Fast" },
                { value: "balanced", label: "Balanced" },
                { value: "accurate", label: "Accurate" },
              ]}
            />
          </Row>
        </Group>
      </Section>

      <Section title="Language">
        <Group>
          <Row label="Spoken language" description="Fixing the language is faster and more accurate than detecting it.">
            <Select value={m.language} width={200} options={LANGS} onChange={(v) => update((s) => void (s.model.language = v))} />
          </Row>
          <Row label="Translate to English" description="Insert an English translation of whatever you say. Needs a multilingual model." disabled={selectedModelIsEnglishOnly(models, selectedId)}>
            <Toggle checked={m.translate_to_english} onChange={(v) => update((s) => void (s.model.translate_to_english = v))} />
          </Row>
        </Group>
        <Hint>English-only models ignore the language setting.</Hint>
      </Section>
    </div>
  );
}

function selectedModelIsEnglishOnly(models: ModelInfo[], id: string | null): boolean {
  const m = models.find((x) => x.id === id);
  return !!m && !m.multilingual;
}

function EngineBanner({ engine, models }: { engine: EngineStatus | null; models: ModelInfo[] }) {
  const state = engine?.state ?? "unloaded";
  const name = engine?.model_id ? (models.find((x) => x.id === engine.model_id)?.name ?? engine.model_id) : null;
  const backend = engine?.backend ? (BACKEND_LABEL[engine.backend] ?? engine.backend) : null;

  let icon = <Cpu size={16} />;
  let title = "No model loaded";
  let sub: string | null = "Download a model below and select it to start dictating.";
  if (state === "loading") {
    icon = <Loader2 size={16} className="spin" />;
    title = `Loading ${name ?? "model"}…`;
    sub = "First load compiles GPU kernels and can take a few seconds.";
  } else if (state === "ready") {
    icon = <Check size={16} />;
    title = `${name ?? "Model"} is ready`;
    sub = engine?.message ?? null;
  } else if (state === "error") {
    icon = <AlertTriangle size={16} />;
    title = `Couldn't load ${name ?? "the model"}`;
    sub = engine?.message ?? "Try another model or turn off GPU acceleration.";
  }

  return (
    <div className={`banner ${state}`}>
      <div className="banner-icon">{icon}</div>
      <div className="banner-main">
        <div className="banner-title">{title}</div>
        {sub ? <div className="banner-sub">{sub}</div> : null}
      </div>
      {state === "ready" && backend ? <Badge tone="ok">{backend}</Badge> : null}
    </div>
  );
}
