use super::bias::{self, LogitBias};
use super::{auto_threads, Segment, SttEngine, Transcript, TranscribeOptions};
use crate::settings::Decoding;
use std::ffi::c_void;
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

pub struct WhisperEngine {
    /// Kept alive for the lifetime of `state`.
    _ctx: WhisperContext,
    state: WhisperState,
    model_id: String,
    multilingual: bool,
}

pub const SAMPLE_RATE: usize = 16_000;
/// whisper.cpp refuses input shorter than 1 s; we pad to a little more.
const MIN_SAMPLES: usize = SAMPLE_RATE * 6 / 5;
const TAIL_PAD: usize = SAMPLE_RATE / 4;

impl WhisperEngine {
    pub fn load(model_id: &str, path: &Path, use_gpu: bool) -> anyhow::Result<Self> {
        whisper_rs::install_logging_hooks();
        let mut params = WhisperContextParameters::default();
        params.use_gpu(use_gpu);
        params.flash_attn(use_gpu);
        let ctx = WhisperContext::new_with_params(path, params)
            .map_err(|e| anyhow::anyhow!("failed to load model {}: {e}", path.display()))?;
        let state = ctx.create_state().map_err(|e| anyhow::anyhow!("failed to create state: {e}"))?;
        let multilingual = ctx.is_multilingual();
        Ok(Self { _ctx: ctx, state, model_id: model_id.to_string(), multilingual })
    }

    /// Run a tiny inference so Metal/CUDA kernels are compiled before the first real request.
    pub fn warm_up(&mut self) {
        let silence = vec![0.0f32; MIN_SAMPLES];
        let opts = TranscribeOptions {
            language: "en".into(),
            translate: false,
            initial_prompt: None,
            decoding: Decoding::Fast,
            threads: 0,
            bias_words: Vec::new(),
            bias_strength: 0.0,
        };
        let _ = self.transcribe(&silence, &opts);
    }

    /// Tokenize every spelling of the vocabulary into a boost trie. `None` when
    /// boosting is off or nothing tokenizes.
    fn build_bias(&self, opts: &TranscribeOptions) -> Option<Box<LogitBias>> {
        if !(opts.bias_strength > 0.0) || opts.bias_words.is_empty() {
            return None;
        }
        let mut seqs = Vec::new();
        for w in opts.bias_words.iter().take(300) {
            for s in bias::spellings(w) {
                // Spellings longer than MAX_TOKENS fail to tokenize and are skipped.
                if let Ok(toks) = self._ctx.tokenize(&s, bias::MAX_TOKENS) {
                    if !toks.is_empty() {
                        seqs.push(toks);
                    }
                }
            }
        }
        LogitBias::build(seqs, opts.bias_strength, self._ctx.n_vocab().max(0) as usize, self._ctx.token_eot())
            .map(Box::new)
    }

    /// Token ids and their text for `text`; diagnostics only.
    pub fn debug_tokens(&self, text: &str) -> Vec<(i32, String)> {
        self._ctx
            .tokenize(text, bias::MAX_TOKENS)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (t, self._ctx.token_to_str(t).map(|s| s.to_string()).unwrap_or_default()))
            .collect()
    }
}

impl SttEngine for WhisperEngine {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn is_multilingual(&self) -> bool {
        self.multilingual
    }

    fn transcribe(&mut self, audio: &[f32], opts: &TranscribeOptions) -> anyhow::Result<Transcript> {
        let mut pcm: Vec<f32> = Vec::with_capacity(audio.len().max(MIN_SAMPLES) + TAIL_PAD);
        pcm.extend_from_slice(audio);
        pcm.resize(pcm.len() + TAIL_PAD, 0.0);
        if pcm.len() < MIN_SAMPLES {
            pcm.resize(MIN_SAMPLES, 0.0);
        }

        let strategy = match opts.decoding {
            Decoding::Fast => SamplingStrategy::Greedy { best_of: 1 },
            Decoding::Balanced => SamplingStrategy::Greedy { best_of: 5 },
            Decoding::Accurate => SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 },
        };
        let mut params = FullParams::new(strategy);
        let threads = if opts.threads == 0 { auto_threads() } else { opts.threads };
        params.set_n_threads(threads as i32);
        params.set_translate(opts.translate && self.multilingual);
        let lang_owned;
        if self.multilingual {
            if opts.language == "auto" || opts.language.is_empty() {
                params.set_language(Some("auto"));
                params.set_detect_language(false);
            } else {
                lang_owned = opts.language.clone();
                params.set_language(Some(lang_owned.as_str()));
            }
        } else {
            params.set_language(Some("en"));
        }
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_token_timestamps(false);
        params.set_no_speech_thold(0.6);
        params.set_entropy_thold(2.4);
        params.set_logprob_thold(-1.0);
        match opts.decoding {
            Decoding::Fast => {
                params.set_temperature(0.0);
                params.set_temperature_inc(0.0);
            }
            _ => {
                params.set_temperature(0.0);
                params.set_temperature_inc(0.2);
            }
        }
        if let Some(prompt) = &opts.initial_prompt {
            if !prompt.trim().is_empty() {
                params.set_initial_prompt(prompt);
            }
        }

        let bias = self.build_bias(opts);
        if let Some(b) = &bias {
            // SAFETY: `b` lives until after `full` returns and whisper.cpp only reads through
            // the pointer from the decoding thread, inside that call.
            unsafe {
                params.set_filter_logits_callback(Some(bias::filter));
                params.set_filter_logits_callback_user_data(&**b as *const LogitBias as *mut c_void);
            }
        }

        let result = self.state.full(params, &pcm);
        drop(bias);
        result.map_err(|e| anyhow::anyhow!("inference failed: {e}"))?;

        let mut segments = Vec::new();
        let mut text = String::new();
        for seg in self.state.as_iter() {
            let s = seg.to_str_lossy().map(|c| c.to_string()).unwrap_or_default();
            let s = s.trim();
            if s.is_empty() {
                continue;
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(s);
            segments.push(Segment {
                start_ms: (seg.start_timestamp().max(0) as u64) * 10,
                end_ms: (seg.end_timestamp().max(0) as u64) * 10,
                text: s.to_string(),
                no_speech_prob: seg.no_speech_probability(),
            });
        }
        let language = if self.multilingual {
            let id = self.state.full_lang_id_from_state();
            whisper_rs::get_lang_str(id).map(|s| s.to_string())
        } else {
            Some("en".into())
        };
        Ok(Transcript { text, segments, language })
    }
}

/// Build the whisper prompt from the user's vocabulary. Whisper honours ~224 prompt
/// tokens; we keep well under that.
pub fn vocabulary_prompt(words: &[String]) -> Option<String> {
    if words.is_empty() {
        return None;
    }
    let mut out = String::new();
    for w in words.iter().take(60) {
        if out.len() + w.len() > 600 {
            break;
        }
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(w);
    }
    if out.is_empty() {
        None
    } else {
        out.push('.');
        Some(out)
    }
}
