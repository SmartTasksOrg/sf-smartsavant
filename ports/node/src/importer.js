'use strict';
// Read council exports written by other tools (the SmartSignal council) into a council result, so that
// the gates, the audit and the exporters work on them. Two shapes are recognised: a council result
// (rounds with views that hold the fields) and a rounds export (views with a "parsed" object).
// A view whose source is "offline" becomes status offline; a full merge that fails synthesisOk becomes
// thin; object contributions are flattened to "key: value" lines; a repeated (step_id, iteration) pair
// gets ".2", ".3", ... appended.
const gates = require('./gates');
const py = require('./py');
const M = require('./model');

const STATUS = new Set(['full', 'recovered', 'offline', 'failed', 'thin', 'no_views']);
const MAX_DEPTH = 20;

/** Unpaired surrogates (which cannot be written as UTF-8) become U+FFFD. */
const clean = (t) => py.replaceLone(t, '�');

function typeName(v) {
  if (v === undefined) return 'undefined';
  if (typeof v === 'function') return 'function';
  if (typeof v !== 'object') return typeof v;
  const c = v.constructor;
  return typeof c === 'function' && c.name ? c.name : 'object';
}

function text(v, depth = 0) {
  if (depth > MAX_DEPTH) return '[nested too deep]';
  if (v === null || v === undefined) return '';
  if (typeof v === 'string') return clean(v);
  if (Array.isArray(v)) return v.map((x) => text(x, depth + 1)).join('; ');
  if (py.isDict(v) && (Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null)) {
    return Object.keys(v).map((k) => `${text(k, depth + 1)}: ${text(v[k], depth + 1)}`).join('\n');
  }
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'number') return Number.isFinite(v) ? py.num(v) : 'float';
  return typeName(v);
}

const asDict = (v) => (py.isDict(v) ? v : {});
const asList = (v) => (Array.isArray(v) ? v : []);

function strs(v) {
  if (v === null || v === undefined) return [];
  if (typeof v === 'string') return gates.blank(v) ? [] : [v];
  if (Array.isArray(v)) return v.map((x) => text(x)).filter((x) => !gates.blank(x));
  return [text(v)];
}

function ideaText(idea) {
  if (typeof idea === 'string') return idea;
  if (py.isDict(idea)) {
    const [name, prob, sol, aud] = ['name', 'problem', 'solution', 'audience'].map((k) => text(py.get(idea, k, '')));
    let s = py.stripChars(`${name}: ${prob}`, ': ');
    if (sol) s += ` -> ${sol}`;
    if (aud) s += ` (audience: ${aud})`;
    return s;
  }
  return text(idea);
}

/** 'smartsavant', 'smartsignal-rounds', 'smartsignal-result' or null. */
function detect(doc) {
  if (!py.isDict(doc)) return null;
  if (py.get(doc, 'schema') === 'smartsavant.council/1') return 'smartsavant';
  const rounds = py.get(doc, 'rounds');
  if (Array.isArray(rounds) && rounds.length && rounds.some((r) => py.isDict(r) && py.has(r, 'views'))) {
    for (const r of rounds) {
      for (const v of asList(py.get(asDict(r), 'views'))) {
        if (py.isDict(v)) return py.has(v, 'parsed') ? 'smartsignal-rounds' : 'smartsignal-result';
      }
    }
    return 'smartsignal-result';
  }
  return null;
}

// Python's `type(x) is int and 0 <= x < 1000`; JSON.parse gives 1.0 as 1, so 1.0 counts as an int here
const smallInt = (x) => py.isInt(x) && x >= 0 && x < 1000;

function fromSmartsignal(doc) {
  const kind = detect(doc);
  if (kind !== 'smartsignal-rounds' && kind !== 'smartsignal-result') {
    throw new Error('not a SmartSignal council export (no rounds with views)');
  }
  const models = new Set();
  const res = M.councilResult({ idea: clean(ideaText(py.get(doc, 'idea', ''))), synthetic: false });
  const seen = new Map();
  const names = [];
  for (const r of doc.rounds) {
    if (!py.isDict(r)) continue;
    let step = text(py.get(r, 'step_id', 'step')) || 'step';
    const iter = py.get(r, 'iteration');
    const rnd = py.get(r, 'round');
    const it = smallInt(iter) ? iter + 1 : smallInt(rnd) ? rnd + 1 : 1;
    const key = JSON.stringify([step, it]);
    seen.set(key, (seen.get(key) || 0) + 1);
    if (seen.get(key) > 1) step = `${step}.${seen.get(key)}`;
    const views = [];
    for (const v of asList(py.get(r, 'views'))) {
      if (!py.isDict(v)) continue;
      const p = asDict(kind === 'smartsignal-rounds' ? py.get(v, 'parsed', v) : v);
      const src = text(py.get(v, 'source', ''));
      if (py.truthy(py.get(v, 'model'))) models.add(text(v.model));
      const w = asDict(py.get(p, 'w5h1'));
      const who = text(py.get(v, 'stakeholder', ''));
      if (!names.includes(who)) names.push(who);
      const w5 = {};
      for (const k of ['who', 'what', 'where', 'when', 'why', 'how']) w5[k] = text(py.get(w, k, ''));
      views.push(M.stakeholderView({
        stakeholder: who, role: text(py.get(v, 'role', '')), status: src === 'offline' ? 'offline' : 'ok',
        w5h1: w5, done_so_far: text(py.get(p, 'done_so_far')), missing: strs(py.get(p, 'missing')),
        devils_advocate: text(py.get(p, 'devils_advocate')), contribution: text(py.get(p, 'contribution')),
        sources: strs(py.get(p, 'sources')).filter((s) => py.len(s) <= 200),
      }));
    }
    const j = asDict(py.get(r, 'juror'));
    if (py.truthy(py.get(j, 'model'))) models.add(text(j.model));
    let st = text(py.get(j, 'synthesis_status', '')) || 'failed';
    st = STATUS.has(st) ? st : 'failed';
    const resolved = strs(py.get(j, 'resolved'));
    const juror = M.jurorMerge({
      synthesis_status: st, merge_summary: text(py.get(j, 'merge_summary')),
      decision: text(py.get(j, 'decision')) || (resolved.length ? resolved[0] : ''),
      resolved, open_issues: strs(py.get(j, 'open_issues')),
      next_steps: strs(py.get(j, 'next_steps')), reversals: strs(py.get(j, 'reversals')),
      spec_delta: text(py.get(j, 'spec_delta')),
      loop_recommended: py.get(j, 'loop_recommended') === true,
    });
    if (st === 'full' && !gates.synthesisOk(juror)) {
      juror.synthesis_status = 'thin';
      juror.error = 'imported as full, but it fails the synthesis gate';
    }
    res.rounds.push(M.councilRound({
      step_id: step, step_title: text(py.get(r, 'step_title', step)) || step, iteration: it,
      question: text(py.get(r, 'question', py.get(r, 'step_title', ''))), views, juror,
    }));
    res.decision_tree.push(M.decisionNode({
      step_id: step, iteration: it, branches: views.filter((x) => x.status === 'ok').map((x) => x.contribution),
      decision: juror.decision, next_steps: [...juror.next_steps],
    }));
    for (const x of views) {
      if (x.status !== 'ok') continue;
      const fv = {
        stakeholder: x.stakeholder, contribution: x.contribution, devils_advocate: x.devils_advocate,
        done_so_far: x.done_so_far, missing: x.missing, sources: x.sources,
      };
      for (const fl of gates.flagUnsourced(fv, [])) res.flags.push({ ...fl, step_id: step, iteration: it });
    }
    if (juror.synthesis_status === 'full') {
      for (const d of juror.resolved) if (!res.locked_decisions.includes(d)) res.locked_decisions.push(d);
    }
  }
  const decs = py.get(doc, 'decisions');
  const anyFull = res.rounds.some((r) => r.juror.synthesis_status === 'full');
  if (anyFull && Array.isArray(decs) && decs.length && decs.every(py.isDict)) {
    res.locked_decisions = decs.map((d) => `${text(py.get(d, 'topic', ''))}: ${text(py.get(d, 'decision', ''))}`)
      .filter((x) => !gates.isFallback(x));
  }
  const sh = asList(py.get(doc, 'stakeholders')).map((s) => (py.isDict(s) ? text(py.get(s, 'name', '')) : text(s)));
  res.stakeholders = sh.length ? sh : names;
  res.adapter = models.size ? 'smartsignal:' + py.sorted([...models]).join(',') : 'smartsignal';
  return res;
}

module.exports = { fromSmartsignal, detect, ideaText, text };
