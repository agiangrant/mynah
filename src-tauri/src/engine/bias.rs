//! Decode-time vocabulary boosting.
//!
//! Whisper has no way to "add" words, so we do shallow fusion: every vocabulary
//! spelling is tokenized and stored in a prefix trie. whisper.cpp calls [`filter`]
//! with the raw logits before each token is sampled, and we add a bonus to the
//! tokens that would continue a vocabulary word from the tokens already emitted.
//!
//! Nothing is ever penalised, so the bonus only decides near-ties: a word the
//! audio does not support stays out of reach, and speech that contains no
//! vocabulary word is left exactly as the model would have written it.

use std::ffi::c_void;
use std::os::raw::c_int;
use whisper_rs_sys::{whisper_context, whisper_state, whisper_token_data};

pub type Token = i32;

/// Longest token sequence tracked per spelling; longer ones are ignored.
pub const MAX_TOKENS: usize = 16;
/// A multi-token word's first token is often a common sub-word (" Fl" for
/// "Flickr"), so it only gets this fraction of the bonus. Once the audio has
/// committed to it, the remaining tokens get the full bonus.
const FIRST_TOKEN_FACTOR: f32 = 0.5;
/// A token is only boosted if the bonus brings it within this many nats of the
/// current best logit. Tokens that could not win anyway are left untouched, so
/// the distribution the model produced is unchanged whenever no vocabulary word
/// is in contention.
const REACH: f32 = 2.0;

#[derive(Default)]
struct Node {
    children: Vec<(Token, usize)>,
}

pub struct LogitBias {
    nodes: Vec<Node>,
    /// bonus for every word's first token (full for single-token words, reduced otherwise)
    root: Vec<(Token, f32)>,
    boost: f32,
    max_len: usize,
    n_vocab: usize,
    /// first special token id; anything at or above it ends a word
    eot: Token,
}

impl LogitBias {
    pub fn build(sequences: Vec<Vec<Token>>, boost: f32, n_vocab: usize, eot: Token) -> Option<Self> {
        if !(boost > 0.0) || n_vocab == 0 {
            return None;
        }
        let mut nodes = vec![Node::default()];
        let mut root: Vec<(Token, f32)> = Vec::new();
        let mut max_len = 0usize;
        for seq in sequences {
            if seq.is_empty()
                || seq.len() > MAX_TOKENS
                || seq.iter().any(|&t| t < 0 || t as usize >= n_vocab || t >= eot)
            {
                continue;
            }
            max_len = max_len.max(seq.len());
            let first = if seq.len() == 1 { boost } else { boost * FIRST_TOKEN_FACTOR };
            push_max(&mut root, seq[0], first);
            let mut node = 0usize;
            for &t in &seq {
                node = match nodes[node].children.iter().find(|(c, _)| *c == t) {
                    Some(&(_, next)) => next,
                    None => {
                        let idx = nodes.len();
                        nodes.push(Node::default());
                        nodes[node].children.push((t, idx));
                        idx
                    }
                };
            }
        }
        if max_len == 0 {
            return None;
        }
        Some(Self { nodes, root, boost, max_len, n_vocab, eot })
    }

    /// Number of distinct first tokens, i.e. roughly how many words are armed.
    pub fn len(&self) -> usize {
        self.root.len()
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    /// Tokens to boost given the most recent non-special tokens, oldest first.
    pub fn boosts(&self, history: &[Token]) -> Vec<(Token, f32)> {
        let mut out = self.root.clone();
        let n = history.len();
        for k in 1..=n.min(self.max_len - 1) {
            let mut node = 0usize;
            let mut matched = true;
            for &t in &history[n - k..] {
                match self.nodes[node].children.iter().find(|(c, _)| *c == t) {
                    Some(&(_, next)) => node = next,
                    None => {
                        matched = false;
                        break;
                    }
                }
            }
            if matched {
                for &(t, _) in &self.nodes[node].children {
                    push_max(&mut out, t, self.boost);
                }
            }
        }
        out
    }
}

fn push_max(v: &mut Vec<(Token, f32)>, t: Token, b: f32) {
    if let Some(e) = v.iter_mut().find(|(c, _)| *c == t) {
        if b > e.1 {
            e.1 = b;
        }
    } else {
        v.push((t, b));
    }
}

/// Spellings to tokenize for one vocabulary entry. Whisper writes a leading space
/// before every word, so each form gets one; casing variants cover sentence
/// starts and all-lowercase output. The post-processor restores the user's casing.
pub fn spellings(word: &str) -> Vec<String> {
    let w = word.trim();
    if w.is_empty() {
        return Vec::new();
    }
    let mut forms: Vec<String> = vec![w.to_string()];
    let lower = w.to_lowercase();
    if !forms.contains(&lower) {
        forms.push(lower);
    }
    let mut chars = w.chars();
    if let Some(c) = chars.next() {
        if c.is_lowercase() {
            let cap: String = c.to_uppercase().chain(chars).collect();
            if !forms.contains(&cap) {
                forms.push(cap);
            }
        }
    }
    forms.into_iter().map(|f| format!(" {f}")).collect()
}

/// whisper.cpp logits filter. `user_data` must point at a [`LogitBias`] that
/// outlives the `whisper_full` call.
///
/// # Safety
/// Called by whisper.cpp on the decoding thread with `n_tokens` valid entries at
/// `tokens` and `n_vocab` floats at `logits`.
pub unsafe extern "C" fn filter(
    _ctx: *mut whisper_context,
    _state: *mut whisper_state,
    tokens: *const whisper_token_data,
    n_tokens: c_int,
    logits: *mut f32,
    user_data: *mut c_void,
) {
    if user_data.is_null() || logits.is_null() {
        return;
    }
    let bias = &*(user_data as *const LogitBias);
    let n = if tokens.is_null() { 0 } else { n_tokens.max(0) as usize };
    let want = bias.max_len.saturating_sub(1).min(MAX_TOKENS);

    // Collect the trailing run of ordinary text tokens, oldest first.
    let mut buf = [0 as Token; MAX_TOKENS];
    let mut count = 0usize;
    if n > 0 && want > 0 {
        let toks = std::slice::from_raw_parts(tokens, n);
        for td in toks.iter().rev() {
            if count >= want {
                break;
            }
            let id = td.id;
            if id < 0 || id >= bias.eot {
                break;
            }
            count += 1;
            buf[want - count] = id;
        }
    }
    let history = &buf[want - count..want];

    let logits = std::slice::from_raw_parts_mut(logits, bias.n_vocab);
    apply(bias, history, logits);
}

fn apply(bias: &LogitBias, history: &[Token], logits: &mut [f32]) {
    let boosts = bias.boosts(history);
    if boosts.is_empty() {
        return;
    }
    let top = logits.iter().copied().filter(|l| l.is_finite()).fold(f32::NEG_INFINITY, f32::max);
    if !top.is_finite() {
        return;
    }
    for (t, b) in boosts {
        let idx = t as usize;
        if idx < logits.len() {
            let l = &mut logits[idx];
            if l.is_finite() && *l + b >= top - REACH {
                *l += b;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bonus(v: &[(Token, f32)], t: Token) -> Option<f32> {
        v.iter().find(|(c, _)| *c == t).map(|(_, b)| *b)
    }

    #[test]
    fn trie_boosts_only_continuations() {
        // A = [1,2,3], B = [1,5], C = [7]
        let bias = LogitBias::build(vec![vec![1, 2, 3], vec![1, 5], vec![7]], 4.0, 100, 50).unwrap();
        assert_eq!(bias.len(), 2);

        let at_root = bias.boosts(&[]);
        assert_eq!(bonus(&at_root, 1), Some(2.0), "multi-token first token gets half");
        assert_eq!(bonus(&at_root, 7), Some(4.0), "single-token word gets full");
        assert_eq!(bonus(&at_root, 2), None);

        let after_1 = bias.boosts(&[1]);
        assert_eq!(bonus(&after_1, 2), Some(4.0));
        assert_eq!(bonus(&after_1, 5), Some(4.0));
        assert_eq!(bonus(&after_1, 3), None);

        let after_12 = bias.boosts(&[9, 1, 2]);
        assert_eq!(bonus(&after_12, 3), Some(4.0));
        assert_eq!(bonus(&after_12, 5), None);

        // a broken prefix falls back to the root bonuses only
        let broken = bias.boosts(&[1, 9]);
        assert_eq!(bonus(&broken, 2), None);
        assert_eq!(bonus(&broken, 1), Some(2.0));

        // a completed word arms nothing beyond the root
        let done = bias.boosts(&[1, 2, 3]);
        assert_eq!(done.len(), at_root.len());
    }

    #[test]
    fn build_rejects_bad_input() {
        assert!(LogitBias::build(vec![vec![1]], 0.0, 100, 50).is_none());
        assert!(LogitBias::build(vec![], 3.0, 100, 50).is_none());
        // special tokens and out-of-range ids are dropped
        assert!(LogitBias::build(vec![vec![1, 60], vec![120]], 3.0, 100, 50).is_none());
        let ok = LogitBias::build(vec![vec![1, 60], vec![2]], 3.0, 100, 50).unwrap();
        assert_eq!(ok.len(), 1);
    }

    #[test]
    fn apply_only_touches_tokens_in_reach() {
        // word A = [1,2], word C = [7]
        let bias = LogitBias::build(vec![vec![1, 2], vec![7]], 4.0, 10, 50).unwrap();
        let mut logits = vec![-30.0f32; 10];
        logits[0] = 10.0; // model's favourite
        logits[7] = 5.0; // single-token word, +4 -> 9.0, within reach of 10
        logits[1] = 7.5; // first token of A, +2 -> 9.5
        logits[2] = 9.0; // continuation of A, but history is empty: no bonus
        logits[3] = f32::NEG_INFINITY; // suppressed by whisper.cpp
        apply(&bias, &[], &mut logits);
        assert_eq!(logits[7], 9.0);
        assert_eq!(logits[1], 9.5);
        assert_eq!(logits[2], 9.0);
        assert_eq!(logits[0], 10.0);
        assert!(logits[3].is_infinite());

        // too far below the top: untouched even though it is a vocabulary token
        let mut far = vec![-30.0f32; 10];
        far[0] = 10.0;
        far[7] = 2.0;
        apply(&bias, &[], &mut far);
        assert_eq!(far[7], 2.0);

        // after token 1 the continuation 2 gets the full bonus
        let mut cont = vec![-30.0f32; 10];
        cont[0] = 10.0;
        cont[2] = 7.0;
        apply(&bias, &[1], &mut cont);
        assert_eq!(cont[2], 11.0);
    }

    #[test]
    fn callback_reads_history_and_stops_at_special_tokens() {
        let bias = LogitBias::build(vec![vec![1, 2, 3]], 4.0, 10, 50).unwrap();
        let mut toks: Vec<whisper_token_data> = (0..3).map(|_| unsafe { std::mem::zeroed() }).collect();
        toks[0].id = 1; // before a timestamp token: must not count
        toks[1].id = 60; // special (>= eot)
        toks[2].id = 1;
        let mut logits = vec![-30.0f32; 10];
        logits[0] = 10.0;
        logits[2] = 7.0;
        logits[3] = 7.0;
        unsafe {
            filter(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                toks.as_ptr(),
                toks.len() as c_int,
                logits.as_mut_ptr(),
                &bias as *const LogitBias as *mut c_void,
            );
        }
        assert_eq!(logits[2], 11.0, "continuation of the trailing token 1");
        assert_eq!(logits[3], 7.0, "token 3 would need history [1,2]");

        // a null user_data is a no-op
        let mut untouched = logits.clone();
        unsafe {
            filter(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                toks.as_ptr(),
                toks.len() as c_int,
                untouched.as_mut_ptr(),
                std::ptr::null_mut(),
            );
        }
        assert_eq!(untouched, logits);
    }

    #[test]
    fn spelling_variants() {
        assert_eq!(spellings("Flickr"), vec![" Flickr", " flickr"]);
        assert_eq!(spellings("kubectl"), vec![" kubectl", " Kubectl"]);
        assert_eq!(spellings("SQL"), vec![" SQL", " sql"]);
        assert_eq!(spellings("  Claude Code "), vec![" Claude Code", " claude code"]);
        assert!(spellings("  ").is_empty());
    }
}
