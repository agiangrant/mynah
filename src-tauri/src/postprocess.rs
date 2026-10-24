//! Text clean-up applied to raw model output: hallucination filtering, filler-word
//! removal, capitalisation and custom-vocabulary substitution (exact and phonetic).

use crate::settings::{DictationSettings, VocabEntry};
use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};

/// Whisper emits these (or close variants) on silence / music / noise.
static NON_SPEECH_TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)[\[\(\*][^\]\)\*]{0,60}[\]\)\*]").unwrap());

/// Classic hallucinations on near-silent audio.
const SILENCE_HALLUCINATIONS: &[&str] = &[
    "thank you",
    "thank you.",
    "thanks for watching",
    "thanks for watching!",
    "thank you for watching",
    "thank you for watching.",
    "thank you so much for watching",
    "subtitles by the amara.org community",
    "subtitles by",
    "bye.",
    "bye bye.",
    "you",
    "you.",
    "the end",
    "the end.",
    "so",
    "so.",
    "oh",
    "oh.",
    "um",
    "hmm",
    ".",
    "...",
];

static WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
static SPACE_BEFORE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+([,.;:!?])").unwrap());
static DOUBLE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"([,.;:!?])\s*([,.;:!?])").unwrap());
static ORPHAN_COMMA_START: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[,;:\s]+").unwrap());
static SENTENCE_START: Lazy<Regex> = Lazy::new(|| Regex::new(r"(^|[.!?]\s+)(\p{Ll})").unwrap());
static LONE_I: Lazy<Regex> = Lazy::new(|| Regex::new(r"(^|\s)i(['’]|\s|$)").unwrap());

/// "um" -> "u+m+" so that drawn-out fillers ("ummm", "uhh") match too.
fn elongation_pattern(word: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in word.chars() {
        if prev == Some(c) && c.is_alphabetic() {
            continue; // collapse repeats; the `+` below covers them
        }
        out.push_str(&regex::escape(&c.to_string()));
        if c.is_alphabetic() {
            out.push('+');
        }
        prev = Some(c);
    }
    out
}

/// One custom word with the phonetic signatures of itself and every "sounds like" variant.
struct FuzzyEntry {
    word: String,
    keys: Vec<Phonetic>,
    max_tokens: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct Phonetic {
    /// Soundex-style consonant classes, vowels dropped ("utility" -> "343")
    classes: String,
    /// simplified spelling used for the similarity check ("utility" -> "utiliti")
    full: String,
}

pub struct PostProcessor {
    fillers: Option<Regex>,
    vocab: Vec<(Regex, String)>,
    fuzzy: Vec<FuzzyEntry>,
    auto_capitalize: bool,
    hallucination_filter: bool,
}

impl PostProcessor {
    pub fn new(dictation: &DictationSettings, vocabulary: &[VocabEntry]) -> Self {
        let fillers = if dictation.remove_fillers && !dictation.filler_words.is_empty() {
            let mut words: Vec<String> = dictation
                .filler_words
                .iter()
                .map(|w| elongation_pattern(w.trim()))
                .filter(|w| !w.is_empty())
                .collect();
            // longest first so "uh-huh" wins over "uh"
            words.sort_by_key(|w| std::cmp::Reverse(w.len()));
            // Rust's regex has no lookaround, so the surrounding whitespace is consumed and
            // re-inserted by the replacement closure. A comma before the filler belongs to it.
            let pattern = format!(r"(?i)(?:,\s*|^|\s)(?:{})([,.!?;:]*)(?:\s+|$)", words.join("|"));
            RegexBuilder::new(&pattern).build().ok()
        } else {
            None
        };

        let active: Vec<&VocabEntry> = vocabulary.iter().filter(|v| v.enabled && !v.word.trim().is_empty()).collect();

        // Exact (case-insensitive, whole-word) replacements: pronunciations and the word's own casing.
        let mut vocab = Vec::new();
        for entry in &active {
            let variants = entry
                .pronunciations
                .iter()
                .map(|s| s.trim())
                .chain(std::iter::once(entry.word.trim()))
                .filter(|v| !v.is_empty());
            for v in variants {
                if let Ok(re) = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(v))) {
                    vocab.push((re, entry.word.trim().to_string()));
                }
            }
        }

        // Phonetic fallback: catches "flicker" for "Flickr" without the user guessing the exact misspelling.
        let fuzzy = active
            .iter()
            .map(|v| {
                let variants: Vec<&str> =
                    std::iter::once(v.word.as_str()).chain(v.pronunciations.iter().map(|p| p.as_str())).collect();
                let max_tokens = variants.iter().map(|p| p.split_whitespace().count()).max().unwrap_or(1);
                let mut keys: Vec<Phonetic> = variants.iter().map(|p| phonetic(p)).filter(|k| !k.full.is_empty()).collect();
                keys.dedup();
                FuzzyEntry { word: v.word.trim().to_string(), keys, max_tokens }
            })
            .collect();

        Self { fillers, vocab, fuzzy, auto_capitalize: dictation.auto_capitalize, hallucination_filter: dictation.hallucination_filter }
    }

    /// `low_energy` tells us the audio was near-silent, so aggressive hallucination
    /// filtering is appropriate.
    pub fn process(&self, raw: &str, low_energy: bool) -> String {
        let mut text = raw.trim().to_string();
        if text.is_empty() {
            return text;
        }

        if self.hallucination_filter {
            text = NON_SPEECH_TAGS.replace_all(&text, "").to_string();
            let lowered = text.trim().to_lowercase();
            if lowered.is_empty() {
                return String::new();
            }
            if (low_energy || lowered.split_whitespace().count() <= 3) && SILENCE_HALLUCINATIONS.contains(&lowered.as_str()) {
                return String::new();
            }
            if low_energy {
                // Repeated short phrase on silence ("Thank you. Thank you. Thank you.")
                let words: Vec<&str> = lowered.split_whitespace().collect();
                if words.len() >= 6 {
                    let half = words.len() / 2;
                    if words[..half] == words[half..half * 2] {
                        return String::new();
                    }
                }
            }
        }

        if let Some(re) = &self.fillers {
            // Run a few passes: consecutive fillers share their separating whitespace.
            for _ in 0..3 {
                let next = re
                    .replace_all(&text, |caps: &regex::Captures| {
                        let at_start = caps.get(0).map(|m| m.start() == 0).unwrap_or(false);
                        // keep a sentence terminator that trailed the filler ("that's it, um." -> "that's it.")
                        match caps[1].chars().rev().find(|c| matches!(c, '.' | '!' | '?')) {
                            Some(p) if !at_start => format!("{p} "),
                            _ => " ".to_string(),
                        }
                    })
                    .to_string();
                if next == text {
                    break;
                }
                text = next;
            }
        }

        text = tidy(&text);

        // Capitalise before vocabulary so the user's own spelling and casing are final.
        if self.auto_capitalize {
            text = SENTENCE_START
                .replace_all(&text, |caps: &regex::Captures| format!("{}{}", &caps[1], caps[2].to_uppercase()))
                .to_string();
            text = LONE_I.replace_all(&text, "${1}I${2}").to_string();
        }

        for (re, replacement) in &self.vocab {
            text = re.replace_all(&text, replacement.as_str()).to_string();
        }
        if !self.fuzzy.is_empty() {
            text = self.apply_fuzzy(&text);
        }

        tidy(&text)
    }

    /// Replace transcript words (or short runs of words) that *sound like* a custom word.
    fn apply_fuzzy(&self, text: &str) -> String {
        let tokens: Vec<&str> = text.split(' ').filter(|t| !t.is_empty()).collect();
        let mut out: Vec<String> = Vec::with_capacity(tokens.len());
        let max_window = self.fuzzy.iter().map(|e| e.max_tokens + 2).max().unwrap_or(1).min(4);
        let mut i = 0;
        'outer: while i < tokens.len() {
            for n in (1..=max_window.min(tokens.len() - i)).rev() {
                let window = &tokens[i..i + n];
                // never span a sentence boundary
                if window[..n - 1].iter().any(|t| t.ends_with(['.', '!', '?'])) {
                    continue;
                }
                let joined: String = window.iter().map(|t| core_of(t).1).collect::<Vec<_>>().join("");
                if joined.is_empty() {
                    continue;
                }
                let key = phonetic(&joined);
                if let Some(entry) = self.fuzzy.iter().find(|e| e.keys.iter().any(|k| sounds_alike(&key, k))) {
                    let (prefix, _, _) = core_of(window[0]);
                    let (_, _, suffix) = core_of(window[n - 1]);
                    out.push(format!("{prefix}{}{suffix}", entry.word));
                    i += n;
                    continue 'outer;
                }
            }
            out.push(tokens[i].to_string());
            i += 1;
        }
        out.join(" ")
    }
}

fn tidy(text: &str) -> String {
    let mut t = WS.replace_all(text, " ").to_string();
    t = SPACE_BEFORE_PUNCT.replace_all(&t, "$1").to_string();
    t = DOUBLE_PUNCT.replace_all(&t, "$1").to_string();
    t = ORPHAN_COMMA_START.replace(&t, "").to_string();
    t.trim().to_string()
}

/// Split a token into (leading punctuation, word core, trailing punctuation).
fn core_of(token: &str) -> (&str, &str, &str) {
    let Some(start) = token.find(|c: char| c.is_alphanumeric()) else {
        return (token, "", "");
    };
    let last = token.rfind(|c: char| c.is_alphanumeric()).unwrap_or(start);
    let end = last + token[last..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    (&token[..start], &token[start..end], &token[end..])
}

/// Simplified spelling + Soundex-style consonant classes. Tolerant of the vowel and
/// voiced/unvoiced confusions ASR makes with names ("flicker" / "Flickr", "kuber needs" / "kubernetes").
fn phonetic(s: &str) -> Phonetic {
    let lower: String = s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let mapped = lower
        .replace("ph", "f")
        .replace("ck", "k")
        .replace("qu", "kw")
        .replace("wh", "w")
        .replace("kn", "n")
        .replace("wr", "r")
        .replace("gh", "")
        .replace('x', "ks")
        .replace('y', "i")
        .replace('q', "k")
        .replace('c', "k")
        .replace('z', "s");
    let mut full = String::with_capacity(mapped.len());
    let mut prev = '\0';
    for ch in mapped.chars() {
        if ch != prev {
            full.push(ch);
        }
        prev = ch;
    }
    let classes: String = full
        .chars()
        .filter_map(|c| match c {
            'b' | 'f' | 'p' | 'v' => Some('1'),
            'g' | 'j' | 'k' | 's' => Some('2'),
            'd' | 't' => Some('3'),
            'l' => Some('4'),
            'm' | 'n' => Some('5'),
            'r' => Some('6'),
            _ => None, // vowels, h, w and digits do not count
        })
        .collect();
    Phonetic { classes, full }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn similarity(a: &str, b: &str) -> f32 {
    let max = a.chars().count().max(b.chars().count());
    if max == 0 {
        return 1.0;
    }
    1.0 - levenshtein(a, b) as f32 / max as f32
}

/// Same consonant skeleton and a spelling similarity that scales with how much
/// skeleton there is to go on (short words must be nearly identical).
fn sounds_alike(heard: &Phonetic, target: &Phonetic) -> bool {
    if heard.full == target.full {
        return true;
    }
    if heard.classes != target.classes {
        return false;
    }
    let min_ratio = match target.classes.len() {
        0 | 1 => 0.85,
        2 => 0.75,
        _ => 0.6,
    };
    similarity(&heard.full, &target.full) >= min_ratio
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DictationSettings;

    fn pp(fillers: bool, vocab: Vec<VocabEntry>) -> PostProcessor {
        let mut d = DictationSettings::default();
        d.remove_fillers = fillers;
        PostProcessor::new(&d, &vocab)
    }

    fn entry(word: &str, sounds: &[&str]) -> VocabEntry {
        VocabEntry { id: word.into(), word: word.into(), pronunciations: sounds.iter().map(|s| s.to_string()).collect(), enabled: true }
    }

    #[test]
    fn removes_fillers() {
        let p = pp(true, vec![]);
        assert_eq!(p.process("Um, so I think, uh, we should go.", false), "So I think we should go.");
        assert_eq!(p.process("Hmm. Okay um let's do it", false), "Okay let's do it");
        assert_eq!(p.process("Humming along", false), "Humming along");
        assert_eq!(p.process("That's it, um.", false), "That's it.");
        assert_eq!(p.process("the sum of all umbrellas", false), "The sum of all umbrellas");
        assert_eq!(p.process("Um. Uh. Hello there", false), "Hello there");
        assert_eq!(p.process("Ummm, so we should, uhh, go. Errr, now.", false), "So we should go. Now.");
    }

    #[test]
    fn applies_vocabulary_exact() {
        let p = pp(false, vec![entry("Mynah", &["my nah", "minah"])]);
        assert_eq!(p.process("open my nah and type", false), "Open Mynah and type");
        assert_eq!(p.process("I love minah.", false), "I love Mynah.");
        let p = pp(false, vec![entry("GitHub", &[])]);
        assert_eq!(p.process("push it to github now", false), "Push it to GitHub now");
    }

    #[test]
    fn applies_vocabulary_phonetically() {
        // the real-world case: user typed phonetic hints, model emitted a dictionary word
        let p = pp(false, vec![entry("Flickr", &["flicker", "flick her"])]);
        assert_eq!(p.process("Flicker.", false), "Flickr.");
        assert_eq!(p.process("Upload it to flicker, then watch the candle flickers.", false), "Upload it to Flickr, then watch the candle flickers.");
        // works with no hints at all
        let p = pp(false, vec![entry("Flickr", &[])]);
        assert_eq!(p.process("we use flicker for photos", false), "We use Flickr for photos");
        // multi-word mis-hearings collapse into one word
        let p = pp(false, vec![entry("Kubernetes", &[])]);
        assert_eq!(p.process("update the kuber needs config", false), "Update the Kubernetes config");
        // common short words are left alone
        let p = pp(false, vec![entry("Mynah", &[])]);
        assert_eq!(p.process("the mine was deep and many came", false), "The mine was deep and many came");
        assert_eq!(p.process("say minah please", false), "Say Mynah please");
    }

    #[test]
    fn filters_hallucinations() {
        let p = pp(true, vec![]);
        assert_eq!(p.process("[BLANK_AUDIO]", true), "");
        assert_eq!(p.process("Thank you.", true), "");
        assert_eq!(p.process("(upbeat music) Hello there", false), "Hello there");
        assert_eq!(p.process("Thank you for the detailed report.", false), "Thank you for the detailed report.");
    }

    #[test]
    fn capitalizes() {
        let p = pp(false, vec![]);
        assert_eq!(p.process("hello. how are you? i'm fine", false), "Hello. How are you? I'm fine");
    }
}
