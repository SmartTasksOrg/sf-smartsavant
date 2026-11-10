'use strict';
// Audit a council run (or its stories, or its trainset) before anyone builds or trains from it.
// The verdict is "build-ready" only when no finding has severity "error". Codes (stable):
//   AUDIT-NO-SYNTHESIS     error    a phase's last merge is not a full synthesis
//   AUDIT-OFFLINE-VIEWS    error    views from an offline template (warning when fewer than half)
//   AUDIT-FAILED-VIEWS     warning  views the adapter never delivered validly
//   AUDIT-FALLBACK-TEXT    error    fallback text in summaries, decisions or spec deltas
//   AUDIT-TEMPLATE-VIEWS   error    the same view text repeated across 3 or more phases
//   AUDIT-GENERIC-STEPS    warning  next steps or story titles that name nothing (error when all are)
//   AUDIT-PLACEHOLDER-CODE warning  "TODO" / return {'ok': True} code presented as content
//   AUDIT-UNSOURCED        warning  figures without a source, or sources the run did not provide
//   AUDIT-COMPLIANCE       error    a compliance rule fired
//   AUDIT-SYNTHETIC        error    a synthetic run (stub, demo)
//   AUDIT-TRAINSET-CONTAMINATED error  trainset rows that teach fallback or placeholder output
//   AUDIT-TRAINSET-UNREADABLE   error  trainset rows that are not objects with an output or messages
// A council is not taken at its word: a merge recorded as full that fails synthesisOk counts as not
// decided, and the adapters stub, scripted and demo* count as synthetic whatever the file says.
const fs = require('fs');
const gates = require('./gates');
const py = require('./py');
const { health } = require('./engine');

const MAX_EXAMPLES = 5;

function finding(code, severity, count, detail, examples = []) {
  return { code, severity, count, detail, examples: examples.slice(0, MAX_EXAMPLES).map((e) => py.head(py.str(e), 200)) };
}

const SYNTHETIC_ADAPTERS = ['stub', 'scripted'];

const decided = (r) => r.juror.synthesis_status === 'full' && gates.synthesisOk(r.juror);

function auditResult(res) {
  const h = health(res);
  const out = [];
  const lastMap = new Map();
  for (const r of res.rounds) lastMap.set(r.step_id, r);
  const last = [...lastMap.values()];
  const undecided = last.filter((r) => !decided(r)).map((r) => `${r.step_id}: ${r.juror.synthesis_status}`
    + (r.juror.synthesis_status !== 'full' ? '' : ' (recorded, but it fails the synthesis gate)'));
  if (undecided.length) {
    out.push(finding('AUDIT-NO-SYNTHESIS', 'error', undecided.length,
      `${undecided.length} of ${last.length} phases have no full synthesis`, undecided));
  }
  const pairs = res.rounds.flatMap((r) => r.views.map((v) => [r, v]));
  const off = pairs.filter(([, v]) => v.status === 'offline').map(([r, v]) => `${r.step_id}/${v.stakeholder}`);
  if (off.length) {
    const sev = 2 * off.length >= Math.max(1, pairs.length) ? 'error' : 'warning';
    out.push(finding('AUDIT-OFFLINE-VIEWS', sev, off.length, `${off.length} of ${pairs.length} views are offline templates`, off));
  }
  const failed = pairs.filter(([, v]) => v.status === 'failed').map(([r, v]) => `${r.step_id}/${v.stakeholder}: ${v.error}`);
  if (failed.length) out.push(finding('AUDIT-FAILED-VIEWS', 'warning', failed.length, `${failed.length} views were never valid`, failed));
  const fb = [];
  for (const r of res.rounds) {
    const j = r.juror;
    const fields = [['merge_summary', j.merge_summary], ['decision', j.decision], ['spec_delta', j.spec_delta]];
    for (const k of ['resolved', 'next_steps', 'open_issues']) j[k].forEach((t, i) => fields.push([`${k}[${i}]`, t]));
    for (const [name, t] of fields) {
      if (gates.isFallback(t)) fb.push(`${r.step_id}.${name}: ${py.head(t, 80)}`);
    }
  }
  if (fb.length) out.push(finding('AUDIT-FALLBACK-TEXT', 'error', fb.length, 'fallback text where a decision should be', fb));
  // the same devil's-advocate or contribution text in several phases: a template
  const byText = new Map();
  for (const r of res.rounds) {
    for (const v of r.views) {
      for (const t of [v.devils_advocate, v.contribution]) {
        const sq = gates.squash(t);
        if (py.len(sq) >= 20) {
          const key = JSON.stringify([v.stakeholder, sq]);
          if (!byText.has(key)) byText.set(key, { who: v.stakeholder, text: sq, steps: new Set() });
          byText.get(key).steps.add(r.step_id.split('.')[0]);
        }
      }
    }
  }
  const tmpl = [...byText.values()].filter((e) => e.steps.size >= 3).map((e) => `${e.who}: ${py.head(e.text, 60)}`);
  if (tmpl.length) out.push(finding('AUDIT-TEMPLATE-VIEWS', 'error', tmpl.length, 'the same view text appears in 3 or more phases', tmpl));
  const steps = last.flatMap((r) => r.juror.next_steps);
  const gen = steps.filter((s) => gates.isGeneric(s));
  if (gen.length) {
    const sev = gen.length === steps.length ? 'error' : 'warning';
    out.push(finding('AUDIT-GENERIC-STEPS', sev, gen.length, `${gen.length} of ${steps.length} next steps name nothing`, gen));
  }
  const ph = pairs.filter(([, v]) => gates.hasPlaceholderCode(v.contribution)).map(([r, v]) => `${r.step_id}/${v.stakeholder}`)
    .concat(res.rounds.filter((r) => gates.hasPlaceholderCode(r.juror.spec_delta)).map((r) => `${r.step_id}.spec_delta`));
  if (ph.length) out.push(finding('AUDIT-PLACEHOLDER-CODE', 'warning', ph.length, 'placeholder code presented as content', ph));
  if (res.flags.length) {
    out.push(finding('AUDIT-UNSOURCED', 'warning', res.flags.length, 'figures without a source, or unknown sources',
      res.flags.map((x) => {
        const fig = py.get(x, 'figure', null);
        return `${py.str(py.get(x, 'step_id', null))}/${py.str(py.get(x, 'stakeholder', null))}: `
          + py.str(py.truthy(fig) ? fig : py.get(x, 'source', null));
      })));
  }
  const text = [res.idea].concat(res.rounds.flatMap((r) => [r.juror.merge_summary, r.juror.decision, r.juror.spec_delta,
    ...r.views.map((v) => v.contribution)])).join(' ');
  const comp = gates.complianceBlock(text);
  if (comp.length) out.push(finding('AUDIT-COMPLIANCE', 'error', comp.length, 'compliance rules fired', comp));
  if (res.synthetic || SYNTHETIC_ADAPTERS.includes(res.adapter) || String(res.adapter).startsWith('demo')) out.push(finding('AUDIT-SYNTHETIC', 'error', 1, `adapter ${res.adapter} is synthetic: not a real deliberation`));
  return report('council', h, out);
}

function auditStories(doc) {
  const stories = py.isDict(doc) ? py.get(doc, 'stories', []) : [];
  const titles = stories.filter(py.isDict).map((s) => py.get(s, 'title', ''));
  const gen = titles.filter((t) => gates.isGeneric(t));
  const blocked = stories.filter((s) => py.isDict(s) && py.get(s, 'status') === 'blocked').map((s) => py.get(s, 'story_id', ''));
  const out = [];
  if (gen.length) {
    const sev = gen.length === titles.length ? 'error' : 'warning';
    out.push(finding('AUDIT-GENERIC-STEPS', sev, gen.length, `${gen.length} of ${titles.length} story titles name nothing`, gen));
  }
  if (blocked.length) {
    out.push(finding('AUDIT-NO-SYNTHESIS', 'error', blocked.length, `${blocked.length} stories are blocked (no decision)`, blocked));
  }
  if (!titles.length) out.push(finding('AUDIT-NO-SYNTHESIS', 'error', 0, 'no stories'));
  return report('stories', { stories: titles.length }, out);
}

function rowText(row) {
  if (!py.isDict(row)) return '';
  const msgs = py.get(row, 'messages');
  if (Array.isArray(msgs)) {
    return msgs.filter((m) => py.isDict(m) && py.get(m, 'role') === 'assistant').map((m) => py.str(py.get(m, 'content', ''))).join(' ');
  }
  return py.str(py.get(row, 'output', ''));
}

function auditTrainset(rows) {
  const bad = [];
  const unreadable = [];
  const kinds = new Map(); // a Map: kinds such as "constructor" or "__proto__" are plain data
  rows.forEach((row, i) => {
    if (!py.isDict(row) || !(typeof py.get(row, 'output') === 'string' || Array.isArray(py.get(row, 'messages')))) {
      unreadable.push(`row ${i}: not an object with an output or messages`);
      return;
    }
    const meta = py.get(row, 'meta', {});
    let k = py.isDict(meta) ? py.get(meta, 'kind') : null;
    k = typeof k === 'string' ? k : 'unknown';
    kinds.set(k, (kinds.get(k) || 0) + 1);
    const t = rowText(row);
    const why = [['fallback', gates.isFallback(t)], ['placeholder code', gates.hasPlaceholderCode(t)]].filter(([, x]) => x).map(([n]) => n);
    if (why.length) bad.push(`row ${i} (${k}): ${why.join(', ')}`);
  });
  const out = [];
  if (unreadable.length) {
    out.push(finding('AUDIT-TRAINSET-UNREADABLE', 'error', unreadable.length,
      `${unreadable.length} of ${rows.length} rows cannot be used`, unreadable));
  }
  if (bad.length) {
    out.push(finding('AUDIT-TRAINSET-CONTAMINATED', 'error', bad.length,
      `${bad.length} of ${rows.length} rows teach fallback or placeholder output`, bad));
  }
  if (!rows.length) out.push(finding('AUDIT-TRAINSET-CONTAMINATED', 'error', 0, 'no rows'));
  return report('trainset', { rows: rows.length, kinds: py.sorted([...kinds.keys()]).map((x) => [x, kinds.get(x)]) }, out);
}

const ORDER = { error: 0, warning: 1, info: 2 };

function report(kind, summary, findings) {
  findings.sort((a, b) => (ORDER[a.severity] - ORDER[b.severity]) || py.cmpStr(a.code, b.code));
  const errors = findings.filter((f) => f.severity === 'error').length;
  return { kind, summary, findings, verdict: errors === 0 ? 'build-ready' : 'not build-ready' };
}

const MAX_ROWS = 200000;              // rows read
const MAX_BYTES = 256 * 1024 * 1024; // characters read (Python counts characters of the decoded lines)
const MAX_LINE = 1024 * 1024;        // longer lines count as unreadable rows
const MAX_LINES = 1000000;           // lines read, empty ones included

/** Parsed rows (null for a line that is not JSON), stopping at the row, size and line budgets. */
function readJsonl(lines, budget = [MAX_ROWS, MAX_BYTES, MAX_LINES]) {
  const rows = [];
  for (let line of lines) {
    budget[1] -= py.len(line);
    budget[2] -= 1;
    if (budget[1] < 0 || budget[0] <= 0 || budget[2] < 0) break;
    line = line.replace(/^[ \t\r\n]+|[ \t\r\n]+$/g, '');
    if (!line) continue;
    budget[0] -= 1;
    if (py.len(line) > MAX_LINE) {
      rows.push(null);
      continue;
    }
    try {
      rows.push(JSON.parse(line));
    } catch (e) {
      rows.push(null);
    }
  }
  return rows;
}

/** Audit one file: a council JSON (this package or SmartSignal), a stories JSON or a .jsonl trainset.
 * Zip archives are not read by this port. */
function auditPath(file) {
  const { detect, fromSmartsignal } = require('./importer');
  const { resultFromDict } = require('./model');
  const { checkCouncil } = require('./schema');
  if (file.endsWith('.zip')) throw new Error('zip trainsets are read by the Python reference only');
  const body = fs.readFileSync(file, 'utf8');
  if (file.endsWith('.jsonl')) return auditTrainset(readJsonl(body.replace(/\r\n?/g, '\n').split(/(?<=\n)/)));
  const doc = JSON.parse(body);
  const kind = detect(doc);
  if (kind === 'smartsavant') {
    const errs = checkCouncil(doc);
    if (errs.length) throw new Error('not a valid council file: ' + errs.slice(0, 3).join('; '));
    return auditResult(resultFromDict(doc));
  }
  if (kind) {
    const rep = auditResult(fromSmartsignal(doc));
    rep.source = kind;
    return rep;
  }
  if (py.isDict(doc) && Array.isArray(py.get(doc, 'stories'))) return auditStories(doc);
  throw new Error('not a council result, a council export, a stories file or a trainset');
}

const ESC_RE = /[\x00-\x1f\x7f-\x9f‪-‮⁦-⁩]|[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/g;

/** Text safe for one terminal line: control characters (newlines included), bidirectional overrides and
 * unpaired surrogates shown as escapes. */
function esc(text) {
  return py.str(text).replace(ESC_RE, (c) => {
    const n = c.charCodeAt(0);
    return n < 0x100 ? '\\x' + n.toString(16).padStart(2, '0') : '\\u' + n.toString(16).padStart(4, '0');
  });
}

/** The audit report as terminal text, as the Python command line prints it. */
function render(rep, name = '') {
  const src = py.truthy(py.get(rep, 'source')) ? ', ' + py.str(rep.source) : '';
  const lines = [`${esc(name)}: ${rep.verdict.toUpperCase()} (${rep.kind}${src})`,
    `  ${py.jsonDumps(rep.summary, true, true)}`];
  for (const f of rep.findings) {
    lines.push(`  [${f.severity}] ${f.code}: ${esc(f.detail)}`);
    for (const e of f.examples) lines.push(`      - ${esc(e)}`);
  }
  return lines.join('\n');
}

module.exports = { auditResult, auditStories, auditTrainset, auditPath, readJsonl, esc, render, MAX_EXAMPLES };
