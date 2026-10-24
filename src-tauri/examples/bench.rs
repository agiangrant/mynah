//! Offline accuracy / latency check without the UI.
//!
//!   say -o /tmp/s.wav --data-format=LEI16@16000 "hello world"
//!   cargo run --release --example bench -- <model.bin> <audio.wav>... \
//!       [--decoding fast|balanced|accurate] [--vocab word,word] [--boost N] [--no-prompt] [--tokens]
//!
//! `--boost 0` disables decode-time vocabulary boosting; the default is the app's
//! "standard" strength. `--tokens` prints how each vocabulary spelling tokenizes.

use mynah_lib::engine::bias::spellings;
use mynah_lib::engine::whisper::WhisperEngine;
use mynah_lib::engine::{SttEngine, TranscribeOptions};
use mynah_lib::postprocess::PostProcessor;
use mynah_lib::settings::{Decoding, DictationSettings, VocabBoost, VocabEntry};
use std::path::Path;
use std::time::Instant;

fn read_wav_16k_mono(path: &Path) -> anyhow::Result<Vec<f32>> {
    let bytes = std::fs::read(path)?;
    anyhow::ensure!(bytes.len() > 44 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE", "not a WAV file");
    let mut pos = 12;
    let mut channels = 1u16;
    let mut rate = 16_000u32;
    let mut bits = 16u16;
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into()?) as usize;
        let body = &bytes[pos + 8..(pos + 8 + size).min(bytes.len())];
        if id == b"fmt " {
            channels = u16::from_le_bytes(body[2..4].try_into()?);
            rate = u32::from_le_bytes(body[4..8].try_into()?);
            bits = u16::from_le_bytes(body[14..16].try_into()?);
        } else if id == b"data" {
            data = Some(body);
        }
        pos += 8 + size + (size & 1);
    }
    let data = data.ok_or_else(|| anyhow::anyhow!("no data chunk"))?;
    anyhow::ensure!(bits == 16, "expected 16-bit PCM, got {bits}");
    let mut mono: Vec<f32> = data
        .chunks_exact(2 * channels as usize)
        .map(|frame| {
            let sum: f32 = frame
                .chunks_exact(2)
                .map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0)
                .sum();
            sum / channels as f32
        })
        .collect();
    if rate != 16_000 {
        let mut r = mynah_lib::audio::Resampler::new(rate, 16_000);
        let mut out = Vec::new();
        r.process(&mono, &mut out);
        mono = out;
    }
    Ok(mono)
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut positional: Vec<String> = Vec::new();
    let mut decoding = Decoding::Balanced;
    let mut words: Vec<String> = vec!["Kubernetes".into()];
    let mut boost: Option<f32> = None;
    let mut prompt = true;
    let mut show_tokens = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--decoding" => {
                decoding = match args.next().as_deref() {
                    Some("fast") => Decoding::Fast,
                    Some("accurate") => Decoding::Accurate,
                    _ => Decoding::Balanced,
                }
            }
            "--vocab" => {
                words = args
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "--boost" => boost = args.next().and_then(|s| s.parse().ok()),
            "--no-prompt" => prompt = false,
            "--tokens" => show_tokens = true,
            other => positional.push(other.to_string()),
        }
    }
    anyhow::ensure!(
        positional.len() >= 2,
        "usage: bench <model.bin> <audio.wav>... [--decoding fast|balanced|accurate] [--vocab a,b] [--boost N] [--no-prompt] [--tokens]"
    );
    let model = Path::new(&positional[0]);
    let wavs = &positional[1..];

    let t = Instant::now();
    let mut engine = WhisperEngine::load("bench", model, true)?;
    let load_ms = t.elapsed().as_millis();
    let t = Instant::now();
    engine.warm_up();
    let warm_ms = t.elapsed().as_millis();

    let vocab: Vec<VocabEntry> = words
        .iter()
        .map(|w| VocabEntry { id: w.clone(), word: w.clone(), pronunciations: vec![], enabled: true })
        .collect();
    let strength = boost.unwrap_or_else(|| VocabBoost::default().strength());
    if show_tokens {
        for w in &words {
            for s in spellings(w) {
                println!("tokens {s:?} -> {:?}", engine.debug_tokens(&s));
            }
        }
    }
    let opts = TranscribeOptions {
        language: "en".into(),
        translate: false,
        initial_prompt: if prompt { mynah_lib::engine::whisper::vocabulary_prompt(&words) } else { None },
        decoding,
        threads: 0,
        bias_words: words.clone(),
        bias_strength: strength,
    };
    let pp = PostProcessor::new(&DictationSettings::default(), &vocab);
    println!(
        "model={} load_ms={load_ms} warmup_ms={warm_ms} decoding={decoding:?} boost={strength} prompt={prompt}",
        model.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    );

    for wav in wavs {
        let audio = read_wav_16k_mono(Path::new(wav))?;
        let secs = audio.len() as f32 / 16_000.0;
        let t = Instant::now();
        let transcript = engine.transcribe(&audio, &opts)?;
        let infer_ms = t.elapsed().as_millis();
        let cleaned = pp.process(&transcript.text, false);
        let name = Path::new(wav).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        println!("{name} {secs:.1}s {infer_ms}ms\n  raw:   {}\n  clean: {cleaned}", transcript.text);
    }
    Ok(())
}
