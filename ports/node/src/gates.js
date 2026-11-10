'use strict';
// Quality gates: pure functions over plain objects (vectors: spec/conformance/gates.json, figures.json).
// Every pattern is written with ASCII semantics: \s is spelled [ \t\n\r\f\v]; \w, \d and \b are ASCII in
// JavaScript; the i flag is used without u, so only ASCII letters fold against these ASCII patterns.
const py = require('./py');

const S = '[ \\t\\n\\r\\f\\v]';
const UNIT = '%|percent|ms|µs|us|ns|s|sec|secs|seconds?|min|mins|minutes?|h|hrs?|hours?|days?|weeks?|months?|years?|'
  + 'x|k|K|m|M|b|B|bn|mn|GB|MB|KB|TB|GiB|MiB|KiB|TiB|kpps|pps|rps|qps|tps|req/s|tokens?|users?|agents?|'
  + 'USD|EUR|GBP';
const FIG = new RegExp(
  '(?<![\\w.])'
  + `(?:(?<cur>[£$€])${S}?)?`
  + '(?<num>\\d{1,3}(?:,\\d{3})+(?:\\.\\d+)?|\\d+\\.\\d+|\\d+)'
  + `(?:${S}?(?<unit>${UNIT}))?`
  + '(?!\\w)',
  'g',
);
const WS = /[ \t\n\r\f\v]+/;

/** Runs of ASCII whitespace (space, tab, LF, CR, FF, VT) to one space, trimmed. */
function squash(text) {
  return String(text || '').split(WS).filter((w) => w).join(' ');
}

/** True for a non-string, or a string that is empty or made only of [ \t\n\r\f\v] (SEMANTICS section 2).
 * Other Unicode spaces (U+00A0, U+3000, \x1c, ...) count as content. */
const blank = (text) => typeof text !== 'string' || /^[ \t\n\r\f\v]*$/.test(text);

const asciiTrim = (s) => s.replace(/^[ \t\n\r\f\v]+|[ \t\n\r\f\v]+$/g, '');

/** Figures in order of appearance, as written. */
function figures(text) {
  const out = [];
  for (const m of String(text || '').matchAll(FIG)) {
    const g = m.groups;
    if (g.cur !== undefined || g.unit !== undefined || g.num.includes(',') || g.num.includes('.')) out.push(asciiTrim(m[0]));
  }
  return out;
}

// iteration as Python does it: a string gives its characters, an object its keys
const iter = (v) => (typeof v === 'string' ? Array.from(v) : Array.isArray(v) ? v : py.isDict(v) ? Object.keys(v) : []);

function viewText(view) {
  let missing = py.get(view, 'missing', '');
  if (Array.isArray(missing)) missing = missing.map(py.str).join(' ');
  return [py.get(view, 'contribution', ''), py.get(view, 'devils_advocate', ''), py.get(view, 'done_so_far', ''), missing]
    .map(py.str).join(' ');
}

/** Flags for one view: unsourced_figures when it cites no source, then unknown_source per cited id not in
 * allowedSources (when the run allows no sources, every cited id is unknown). */
function flagUnsourced(view, allowedSources) {
  const out = [];
  const who = py.get(view, 'stakeholder', '');
  const src = py.get(view, 'sources', []);
  const cited = (py.truthy(src) ? iter(src) : []).filter(py.isStr);
  if (!cited.length) {
    for (const f of figures(viewText(view))) out.push({ flag: 'unsourced_figures', figure: f, stakeholder: who });
  }
  const allowed = new Set(py.truthy(allowedSources) ? iter(allowedSources) : []);
  for (const s of cited) if (!allowed.has(s)) out.push({ flag: 'unknown_source', source: s, stakeholder: who });
  return out;
}

const GENERIC = [
  new RegExp(`^${S}*advance the [\\w/ -]+ with the decided approach[.!?]*${S}*$`, 'i'),
  new RegExp(`^${S}*[A-Za-z]+${S}+[XYZ][.!?]*${S}*$`),
  new RegExp(`^${S}*(next steps?${S}*:?${S}*)?(tbd|todo|n/?a|none|proceed|continue|next steps?)?[.!?]*${S}*$`, 'i'),
];

const FALLBACK = new RegExp('synthesis failed|(falling|fell|fall) back to the raw panel|returned unparseable output|'
  + 'unparseable output|offline fallback|\\(offline\\)|fallback template', 'i');
const isFallback = (text) => FALLBACK.test(String(text || ''));

/** A next step or story title that names no real component or decision (fallback text included). */
function isGeneric(step) {
  return blank(step) || GENERIC.some((p) => p.test(step)) || isFallback(step);
}

const PLACEHOLDER = new RegExp(
  `#${S}*TODO\\b|//${S}*TODO\\b|return${S}*\\{${S}*['"]ok['"]${S}*:${S}*True${S}*\\}|`
  + `raise NotImplementedError|\\bpass${S}*#${S}*TODO`, 'i',
);
const hasPlaceholderCode = (text) => PLACEHOLDER.test(String(text || ''));

/** A merge that genuinely decided something. */
function synthesisOk(merge) {
  if (py.get(merge, 'synthesis_status', 'full') !== 'full') return false;
  const summary = py.get(merge, 'merge_summary', '');
  const decision = py.get(merge, 'decision', '');
  if (blank(summary) || blank(decision)) return false;
  const resolved = py.get(merge, 'resolved', null);
  const steps = py.get(merge, 'next_steps', null);
  const rs = py.truthy(resolved) ? iter(resolved) : [];
  const ss = py.truthy(steps) ? iter(steps) : [];
  if ([summary, decision, ...rs, ...ss].some(isFallback)) return false;
  return py.truthy(resolved) && ss.some((s) => !isGeneric(s));
}

const DEFAULT_RULES = [
  ['possible trading guidance on non-public information',
    [['non-public', 'nonpublic', 'mnpi', 'insider information'], ['buy', 'sell', 'trade', 'short', 'position']]],
  ['possible credential or secret in the output',
    [['-----begin', 'aws_secret_access_key', 'api_key=', 'password=', 'ghp_', 'github' + '_pat_', 'pypi-ag']]],
];

/** Reasons to block a release of this text ([] when no rule fired). Keyword matching only. */
function complianceBlock(text, rules) {
  const low = String(text || '').toLowerCase();
  const out = [];
  for (const [name, groups] of (rules === undefined || rules === null ? DEFAULT_RULES : rules)) {
    if (groups.every((g) => g.some((w) => low.includes(w)))) out.push(name);
  }
  return out;
}

const SECRET = new RegExp('\\b(api[_-]?key|access[_-]?token|token|secret|password|passwd|signature|sig|key)'
  + `(${S}*[=:]${S}*)([^ \\t\\n\\r\\f\\v&;,"']+)`, 'gi');
const BEARER = new RegExp(`\\b(bearer|basic)${S}+[A-Za-z0-9._~+/=-]{8,}`, 'gi');

/** Error text made safe to record: key=VALUE-style secrets and bearer tokens replaced by ***. */
function redact(text) {
  return String(text || '').replace(SECRET, (m, k, sep) => k + sep + '***').replace(BEARER, (m, k) => k + ' ***');
}

module.exports = {
  redact, squash, blank, figures, flagUnsourced, isGeneric, isFallback, hasPlaceholderCode, synthesisOk, complianceBlock,
  DEFAULT_RULES,
};
