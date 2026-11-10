//! Quality gates (spec/SEMANTICS.md section 5; vectors spec/conformance/gates.json and figures.json).
//!
//! Every pattern of the Python reference is an ASCII pattern (`\s` is `[ \t\n\r\f\v]`, `\w` is
//! `[A-Za-z0-9_]`, case-insensitive matching folds ASCII letters only). The standard library has no
//! regular expressions, so the patterns are written out as small matchers over the UTF-8 bytes: a byte
//! of a multi-byte character is never an ASCII letter, digit or space, which is exactly how the ASCII
//! patterns treat a non-ASCII character. Every match starts and ends on a character boundary.
use crate::json::{obj, Object, Value};
use crate::py;

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c | 0x0b)
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// True when `s` has the lowercase ASCII literal `lit` at byte `i`, ASCII letters folded.
fn has_ci(s: &[u8], i: usize, lit: &str) -> bool {
    let l = lit.as_bytes();
    i + l.len() <= s.len() && s[i..i + l.len()].iter().zip(l).all(|(a, b)| a.to_ascii_lowercase() == *b)
}

fn skip_ws(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && is_ws(s[i]) {
        i += 1;
    }
    i
}

/// Runs of `[ \t\n\r\f\v]` to one space, trimmed.
pub fn squash(text: &str) -> String {
    text.split(|c: char| c.is_ascii() && is_ws(c as u8)).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// True for a string empty or made only of `[ \t\n\r\f\v]`.
pub fn blank_str(s: &str) -> bool {
    s.bytes().all(is_ws)
}

/// True for a non-string, or a blank string.
pub fn blank(v: &Value) -> bool {
    match v {
        Value::Str(s) => blank_str(s),
        _ => true,
    }
}

// ---------------------------------------------------------------- figures

const UNITS: &[&str] = &[
    "%", "percent", "ms", "µs", "us", "ns", "s", "sec", "secs", "seconds", "second", "min", "mins", "minutes",
    "minute", "h", "hrs", "hr", "hours", "hour", "days", "day", "weeks", "week", "months", "month", "years", "year",
    "x", "k", "K", "m", "M", "b", "B", "bn", "mn", "GB", "MB", "KB", "TB", "GiB", "MiB", "KiB", "TiB", "kpps", "pps",
    "rps", "qps", "tps", "req/s", "tokens", "token", "users", "user", "agents", "agent", "USD", "EUR", "GBP",
];
const CURRENCIES: &[&str] = &["£", "$", "€"];

fn digits_from(s: &[u8], i: usize) -> usize {
    s[i.min(s.len())..].iter().take_while(|c| c.is_ascii_digit()).count()
}

/// The ends of the number part starting at `i`, in the order Python's backtracking tries them:
/// `\d{1,3}(?:,\d{3})+(?:\.\d+)? | \d+\.\d+ | \d+`
fn num_ends(s: &[u8], i: usize) -> Vec<usize> {
    let mut out = vec![];
    let d = digits_from(s, i);
    if d == 0 {
        return out;
    }
    for k in (1..=d.min(3)).rev() {
        let mut reps = vec![];
        let mut p = i + k;
        while p + 4 <= s.len() && s[p] == b',' && s[p + 1..p + 4].iter().all(|c| c.is_ascii_digit()) {
            p += 4;
            reps.push(p);
        }
        for &e in reps.iter().rev() {
            if e < s.len() && s[e] == b'.' {
                for f in (1..=digits_from(s, e + 1)).rev() {
                    out.push(e + 1 + f);
                }
            }
            out.push(e);
        }
    }
    for k in (1..=d).rev() {
        let e = i + k;
        if e < s.len() && s[e] == b'.' {
            for f in (1..=digits_from(s, e + 1)).rev() {
                out.push(e + 1 + f);
            }
        }
    }
    for k in (1..=d).rev() {
        out.push(i + k);
    }
    out
}

struct FigMatch {
    end: usize,
    counts: bool,
}

fn fig_at(s: &[u8], i: usize) -> Option<FigMatch> {
    if i > 0 && (is_word(s[i - 1]) || s[i - 1] == b'.') {
        return None;
    }
    let mut starts: Vec<(usize, bool)> = vec![];
    for c in CURRENCIES {
        if s[i..].starts_with(c.as_bytes()) {
            let p = i + c.len();
            if p < s.len() && is_ws(s[p]) {
                starts.push((p + 1, true));
            }
            starts.push((p, true));
        }
    }
    starts.push((i, false));
    let ahead = |e: usize| e >= s.len() || !is_word(s[e]);
    for (at, cur) in starts {
        for n in num_ends(s, at) {
            let cp = s[at..n].iter().any(|c| *c == b',' || *c == b'.');
            for sp in [true, false] {
                let mut p = n;
                if sp {
                    if p < s.len() && is_ws(s[p]) {
                        p += 1;
                    } else {
                        continue;
                    }
                }
                for u in UNITS {
                    if s[p..].starts_with(u.as_bytes()) && ahead(p + u.len()) {
                        return Some(FigMatch { end: p + u.len(), counts: true });
                    }
                }
            }
            if ahead(n) {
                return Some(FigMatch { end: n, counts: cur || cp });
            }
        }
    }
    None
}

/// The numbers in `text` that state a quantity, in order of appearance.
pub fn figures(text: &str) -> Vec<String> {
    let s = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < s.len() {
        match fig_at(s, i) {
            Some(m) => {
                if m.counts {
                    out.push(text[i..m.end].to_string());
                }
                i = m.end;
            }
            None => i += 1,
        }
    }
    out
}

// ---------------------------------------------------------------- flag_unsourced

fn view_text(view: &Object) -> String {
    let empty = Value::Str(String::new());
    let missing = match view.get_or("missing", &empty) {
        Value::Arr(l) => l.iter().map(py::to_str).collect::<Vec<_>>().join(" "),
        other => py::to_str(other),
    };
    [
        py::to_str(view.get_or("contribution", &empty)),
        py::to_str(view.get_or("devils_advocate", &empty)),
        py::to_str(view.get_or("done_so_far", &empty)),
        missing,
    ]
    .join(" ")
}

/// The flags for one view (a JSON object), in order: every figure when the view cites no source
/// (`unsourced_figures`), then every cited source id not in `allowed` (`unknown_source`; when `allowed`
/// is empty every cited id is unknown).
pub fn flag_unsourced(view: &Object, allowed: &Value) -> Vec<Value> {
    let mut out = vec![];
    let empty = Value::Str(String::new());
    let who = view.get_or("stakeholder", &empty).clone();
    let src = view.get_or("sources", &Value::Arr(vec![])).clone();
    let cited: Vec<String> = if py::truthy(&src) {
        py::iter(&src).into_iter().filter_map(|x| x.as_str().map(String::from)).collect()
    } else {
        vec![]
    };
    if cited.is_empty() {
        for f in figures(&view_text(view)) {
            out.push(obj(vec![
                ("flag", "unsourced_figures".into()),
                ("figure", f.into()),
                ("stakeholder", who.clone()),
            ]));
        }
    }
    let allow: Vec<String> = if py::truthy(allowed) {
        py::iter(allowed).into_iter().filter_map(|x| x.as_str().map(String::from)).collect()
    } else {
        vec![]
    };
    for s in cited {
        if !allow.contains(&s) {
            out.push(obj(vec![("flag", "unknown_source".into()), ("source", s.into()), ("stakeholder", who.clone())]));
        }
    }
    out
}

// ---------------------------------------------------------------- generic steps, fallback, placeholders

/// Removes trailing `[ \t\n\r\f\v]*` and then trailing `[.!?]*`.
fn trim_tail(s: &[u8]) -> &[u8] {
    let mut e = s.len();
    while e > 0 && is_ws(s[e - 1]) {
        e -= 1;
    }
    while e > 0 && matches!(s[e - 1], b'.' | b'!' | b'?') {
        e -= 1;
    }
    &s[..e]
}

/// `^\s*advance the [\w/ -]+ with the decided approach[.!?]*\s*$` (case-insensitive)
fn generic_advance(s: &[u8]) -> bool {
    let t = trim_tail(&s[skip_ws(s, 0)..]);
    const HEAD: &str = "advance the ";
    const TAIL: &str = " with the decided approach";
    if t.len() < HEAD.len() + 1 + TAIL.len() || !has_ci(t, 0, HEAD) || !has_ci(t, t.len() - TAIL.len(), TAIL) {
        return false;
    }
    t[HEAD.len()..t.len() - TAIL.len()].iter().all(|&c| is_word(c) || c == b'/' || c == b' ' || c == b'-')
}

/// `^\s*[A-Za-z]+\s+[XYZ][.!?]*\s*$`
fn generic_xyz(s: &[u8]) -> bool {
    let i = skip_ws(s, 0);
    let mut j = i;
    while j < s.len() && s[j].is_ascii_alphabetic() {
        j += 1;
    }
    if j == i {
        return false;
    }
    let k = skip_ws(s, j);
    if k == j || k >= s.len() || !matches!(s[k], b'X' | b'Y' | b'Z') {
        return false;
    }
    trim_tail(&s[k + 1..]).is_empty()
}

const PLACEHOLDER_WORDS: &[&str] =
    &["tbd", "todo", "na", "n/a", "none", "proceed", "continue", "next step", "next steps"];

fn generic_word_tail(t: &[u8]) -> bool {
    let u = trim_tail(t).to_ascii_lowercase();
    u.is_empty() || PLACEHOLDER_WORDS.iter().any(|w| u == w.as_bytes())
}

/// `^\s*(next steps?\s*:?\s*)?(tbd|todo|n/?a|none|proceed|continue|next steps?)?[.!?]*\s*$` (case-insensitive)
fn generic_word(s: &[u8]) -> bool {
    let s = &s[skip_ws(s, 0)..];
    if generic_word_tail(s) {
        return true;
    }
    if !has_ci(s, 0, "next step") {
        return false;
    }
    let mut p = "next step".len();
    if has_ci(s, p, "s") {
        p += 1;
    }
    p = skip_ws(s, p);
    if p < s.len() && s[p] == b':' {
        p += 1;
    }
    generic_word_tail(&s[skip_ws(s, p)..])
}

/// A next step or story title that names nothing (fallback text included); true for a non-string.
pub fn is_generic(step: &Value) -> bool {
    match step {
        Value::Str(s) => is_generic_str(s),
        _ => true,
    }
}

pub fn is_generic_str(s: &str) -> bool {
    let b = s.as_bytes();
    blank_str(s) || generic_advance(b) || generic_xyz(b) || generic_word(b) || is_fallback(s)
}

const FALLBACK: &[&str] = &[
    "synthesis failed",
    "falling back to the raw panel",
    "fell back to the raw panel",
    "fall back to the raw panel",
    "returned unparseable output",
    "unparseable output",
    "offline fallback",
    "(offline)",
    "fallback template",
];

/// Text a tool wrote when the model failed.
pub fn is_fallback(text: &str) -> bool {
    let low = text.to_ascii_lowercase();
    FALLBACK.iter().any(|p| low.contains(p))
}

fn is_fallback_value(v: &Value) -> bool {
    py::truthy(v) && is_fallback(&py::to_str(v))
}

fn word_end(s: &[u8], i: usize) -> bool {
    i >= s.len() || !is_word(s[i])
}

/// "TODO" stubs and `return {'ok': True}` bodies presented as code.
pub fn has_placeholder_code(text: &str) -> bool {
    let s = text.as_bytes();
    let quote = |c: u8| c == b'\'' || c == b'"';
    for i in 0..s.len() {
        match s[i].to_ascii_lowercase() {
            b'#' => {
                let p = skip_ws(s, i + 1);
                if has_ci(s, p, "todo") && word_end(s, p + 4) {
                    return true;
                }
            }
            b'/' => {
                if i + 1 < s.len() && s[i + 1] == b'/' {
                    let p = skip_ws(s, i + 2);
                    if has_ci(s, p, "todo") && word_end(s, p + 4) {
                        return true;
                    }
                }
            }
            b'r' => {
                if has_ci(s, i, "return") {
                    let mut p = skip_ws(s, i + 6);
                    if p < s.len() && s[p] == b'{' {
                        p = skip_ws(s, p + 1);
                        if p + 4 <= s.len() && quote(s[p]) && has_ci(s, p + 1, "ok") && quote(s[p + 3]) {
                            p = skip_ws(s, p + 4);
                            if p < s.len() && s[p] == b':' {
                                p = skip_ws(s, p + 1);
                                if has_ci(s, p, "true") {
                                    p = skip_ws(s, p + 4);
                                    if p < s.len() && s[p] == b'}' {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
                if has_ci(s, i, "raise notimplementederror") {
                    return true;
                }
            }
            b'p' if (i == 0 || !is_word(s[i - 1])) && has_ci(s, i, "pass") => {
                let p = skip_ws(s, i + 4);
                if p < s.len() && s[p] == b'#' && has_ci(s, skip_ws(s, p + 1), "todo") {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// A merge (a JSON object) that genuinely decided something: status full (or none given), a non-blank
/// summary and decision, no fallback text, at least one resolved entry and at least one next step that
/// is not generic.
pub fn synthesis_ok(merge: &Object) -> bool {
    if let Some(st) = merge.get("synthesis_status") {
        if !st.is_str("full") {
            return false;
        }
    }
    let empty = Value::Str(String::new());
    let summary = merge.get_or("merge_summary", &empty);
    let decision = merge.get_or("decision", &empty);
    if blank(summary) || blank(decision) {
        return false;
    }
    let resolved = merge.at("resolved");
    let steps = merge.at("next_steps");
    let rs = if py::truthy(resolved) { py::iter(resolved) } else { vec![] };
    let ss = if py::truthy(steps) { py::iter(steps) } else { vec![] };
    if [summary, decision].into_iter().chain(rs.iter()).chain(ss.iter()).any(is_fallback_value) {
        return false;
    }
    py::truthy(resolved) && ss.iter().any(|s| !is_generic(s))
}

// ---------------------------------------------------------------- compliance and redaction

/// A compliance rule: it fires when the lowercased text holds a word of every group.
#[derive(Clone, Debug)]
pub struct Rule {
    pub name: String,
    pub groups: Vec<Vec<String>>,
}

/// The keyword stubs of the reference (a private build plugs in a real policy).
pub fn default_rules() -> Vec<Rule> {
    let r = |name: &str, groups: &[&[&str]]| Rule {
        name: name.into(),
        groups: groups.iter().map(|g| g.iter().map(|w| w.to_string()).collect()).collect(),
    };
    vec![
        r(
            "possible trading guidance on non-public information",
            &[
                &["non-public", "nonpublic", "mnpi", "insider information"],
                &["buy", "sell", "trade", "short", "position"],
            ],
        ),
        r(
            "possible credential or secret in the output",
            &[&[
                "-----begin",
                "aws_secret_access_key",
                "api_key=",
                "password=",
                "ghp_",
                concat!("github", "_pat_"),
                "pypi-ag",
            ]],
        ),
    ]
}

/// The names of the rules that fire on `text` (`None`: the default rules).
pub fn compliance_block(text: &str, rules: Option<&[Rule]>) -> Vec<String> {
    let defaults;
    let rules = match rules {
        Some(r) => r,
        None => {
            defaults = default_rules();
            &defaults
        }
    };
    let low = py::lower(text);
    rules
        .iter()
        .filter(|r| r.groups.iter().all(|g| g.iter().any(|w| low.contains(w.as_str()))))
        .map(|r| r.name.clone())
        .collect()
}

const SECRET_KEYS: &[&str] = &[
    "api_key",
    "api-key",
    "apikey",
    "access_token",
    "access-token",
    "accesstoken",
    "token",
    "secret",
    "password",
    "passwd",
    "signature",
    "sig",
    "key",
];

/// `\b(api[_-]?key|...|key)(\s*[=:]\s*)([^\s&;,"']+)` at `i` (case-insensitive): the ends of the
/// separator and of the value.
fn secret_at(s: &[u8], i: usize) -> Option<(usize, usize)> {
    if (i > 0 && is_word(s[i - 1])) || i >= s.len() || !is_word(s[i]) {
        return None;
    }
    for k in SECRET_KEYS {
        if !has_ci(s, i, k) {
            continue;
        }
        let p = skip_ws(s, i + k.len());
        if p >= s.len() || (s[p] != b'=' && s[p] != b':') {
            continue;
        }
        let g2 = skip_ws(s, p + 1);
        let mut e = g2;
        while e < s.len() && !is_ws(s[e]) && !b"&;,\"'".contains(&s[e]) {
            e += 1;
        }
        if e > g2 {
            return Some((g2, e));
        }
    }
    None
}

/// `\b(bearer|basic)\s+[A-Za-z0-9._~+/=-]{8,}` at `i` (case-insensitive): the keyword's length and the end.
fn bearer_at(s: &[u8], i: usize) -> Option<(usize, usize)> {
    if i > 0 && is_word(s[i - 1]) {
        return None;
    }
    for k in ["bearer", "basic"] {
        if !has_ci(s, i, k) {
            continue;
        }
        let p = skip_ws(s, i + k.len());
        if p == i + k.len() {
            continue;
        }
        let mut e = p;
        while e < s.len() && (s[e].is_ascii_alphanumeric() || b"._~+/=-".contains(&s[e])) {
            e += 1;
        }
        if e - p >= 8 {
            return Some((k.len(), e));
        }
    }
    None
}

/// Error text made safe to record: `key=VALUE`-style secrets and bearer tokens become `***`.
pub fn redact(text: &str) -> String {
    let s = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if let Some((g2, e)) = secret_at(s, i) {
            out.extend_from_slice(&s[i..g2]);
            out.extend_from_slice(b"***");
            i = e;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    let s = out;
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if let Some((klen, e)) = bearer_at(&s, i) {
            out.extend_from_slice(&s[i..i + klen]);
            out.extend_from_slice(b" ***");
            i = e;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    String::from_utf8(out).expect("matches start and end on character boundaries")
}
