'use strict';
// The council engine: roster -> phases -> stakeholder views -> juror merge -> decision tree.
//   const res = await new Council('...', new StubAdapter()).run();
// Every answer is validated (checkView / checkMerge) and retried; one that still fails is recorded as
// failed with an error, never filled with invented text. Deterministic for a deterministic adapter.
const fs = require('fs');
const path = require('path');
const gates = require('./gates');
const py = require('./py');
const { isAdapter } = require('./adapters/base');
const M = require('./model');
const { NotPlainData, checkMerge, checkView, normalizeView, plain } = require('./schema');

const DEFAULT_ROSTER = ['Developer', 'UX/UI', 'Product Owner', 'Architecture/Infra'];

const DEFAULT_PHASES = [
  ['frame', 'Frame', 'What is the problem, who are the users, and the jobs-to-be-done?'],
  ['concept', 'Concept', 'What is the solution direction and the wedge?'],
  ['design', 'Design', 'What are the user flow, data flow, and experience?'],
  ['build', 'Build', 'What is the technical approach?'],
  ['integrate', 'Integrate', 'How does it fit the target architecture?'],
  ['measure', 'Measure', 'What are the KPIs and success metrics?'],
  ['spec', 'Spec', 'Assemble the final document.'],
];

/** [[step_id, title, question]] from a phases document or file (default: spec/phases.json's content). */
function loadPhases(source) {
  if (source === undefined || source === null) return DEFAULT_PHASES.map((p) => [...p]);
  const doc = typeof source === 'string' ? JSON.parse(fs.readFileSync(path.resolve(source), 'utf8')) : source;
  const ph = py.isDict(doc) ? py.get(doc, 'phases') : null;
  if (!Array.isArray(ph) || !ph.length) throw new Error('a phases file is {"phases": [{"step_id", "title", "question"}, ...]}');
  if (!ph.every(py.isDict)) throw new Error('each phase is an object {"step_id", "title", "question"}');
  return checkPhases(ph.map((p) => [py.get(p, 'step_id'), py.get(p, 'title'), py.get(p, 'question')]));
}

/** [[step_id, title, question]] with unique step ids and non-blank strings, or an Error. A phase may also be
 * given as {step_id, title, question} (converted first). */
function checkPhases(phases) {
  const out = [];
  const seen = new Set();
  for (let p of phases) {
    if (py.isDict(p)) p = [py.get(p, 'step_id'), py.get(p, 'title'), py.get(p, 'question')];
    if (!Array.isArray(p) || p.length !== 3 || !p.every((x) => typeof x === 'string' && !gates.blank(x))) {
      throw new Error('each phase is (step_id, title, question), three non-blank strings');
    }
    if (seen.has(p[0])) throw new Error(`duplicate step_id ${p[0]}`);
    seen.add(p[0]);
    out.push([...p]);
  }
  if (!out.length) throw new Error('at least one phase');
  return out;
}

const NOT_PRINTABLE = '(message not printable)';

function safeString(v) {
  try {
    const s = String(v);
    return typeof s === 'string' ? s : NOT_PRINTABLE;
  } catch (e) {
    return NOT_PRINTABLE;
  }
}

function errorName(e) {
  try {
    const n = e.name;
    if (typeof n === 'string' && n && n !== 'Error') return n;
    const c = e.constructor;
    return typeof c === 'function' && typeof c.name === 'string' && c.name ? c.name : 'Error';
  } catch (x) {
    return 'Error';
  }
}

/** "<Name>: <message>" as Python's _error: lone surrogates in the message become '?', secrets are redacted,
 * then the text is cut to 500 code points. Never throws.
 * An Error gives its name (or its class name when name is the generic "Error"); a thrown value that is
 * not an Error gives "Error: <String(value)>"; a message that cannot be made a string gives
 * "(message not printable)". */
function errorText(e) {
  let name = 'Error';
  let msg;
  if (e instanceof Error) {
    name = errorName(e);
    let m;
    try {
      m = e.message;
    } catch (x) {
      m = undefined;
      msg = NOT_PRINTABLE;
    }
    if (msg === undefined) msg = typeof m === 'string' ? m : safeString(m);
  } else {
    msg = safeString(e);
  }
  msg = py.replaceLone(msg, '?');
  return py.head(gates.redact(`${name}: ${msg}`), 500);
}

const opt = (o, a, b, d) => (o[a] !== undefined && o[a] !== null ? o[a] : o[b] !== undefined && o[b] !== null ? o[b] : d);

class Council {
  /**
   * @param {string} idea
   * @param {object} adapter  anything with view() and merge() (they may be async)
   * @param {object} [options] roster, phases ([[id, title, question]] or [{step_id, title, question}]),
   *   sources, maxLoops (1..10, default 1; max_loops also accepted), retries (0..5, default 1)
   */
  constructor(idea, adapter, options = {}) {
    if (typeof idea !== 'string' || gates.blank(idea)) throw new Error('the idea must be a non-empty string');
    const phaseTexts = (options.phases || []).flatMap((p) => (Array.isArray(p) ? p
      : py.isDict(p) ? [py.get(p, 'step_id'), py.get(p, 'title'), py.get(p, 'question')] : []));
    const texts = [idea, ...(options.roster || []), ...(options.sources || []), ...phaseTexts];
    if (texts.some((x) => typeof x === 'string' && py.hasLone(x))) {
      throw new Error('the idea, roster, phases and sources must not contain unpaired surrogates');
    }
    if (!isAdapter(adapter)) throw new TypeError('adapter must be an LLMAdapter');
    const roster = options.roster;
    this.roster = roster && roster.length ? [...roster] : [...DEFAULT_ROSTER];
    if (new Set(this.roster).size !== this.roster.length || !this.roster.every((r) => typeof r === 'string' && !gates.blank(r))) {
      throw new Error('roster names must be unique non-empty strings');
    }
    const phases = options.phases;
    this.phases = phases && phases.length ? checkPhases([...phases]) : DEFAULT_PHASES.map((p) => [...p]);
    const maxLoops = opt(options, 'maxLoops', 'max_loops', 1);
    const retries = opt(options, 'retries', 'retries', 1);
    if (!Number.isInteger(maxLoops) || maxLoops < 1 || maxLoops > 10) throw new Error('max_loops is an integer from 1 to 10');
    if (!Number.isInteger(retries) || retries < 0 || retries > 5) throw new Error('retries is an integer from 0 to 5');
    this.idea = idea;
    this.adapter = adapter;
    this.sources = [...(options.sources || [])];
    this.maxLoops = maxLoops;
    this.retries = retries;
  }

  /** [answer, ''] or [null, error] after 1 + retries attempts. */
  async _ask(method, check, args) {
    let err = '';
    for (let n = 0; n < 1 + this.retries; n++) {
      let ans;
      try {
        ans = plain(await this.adapter[method](py.clone(args))); // validate exactly what will be used
      } catch (e) { // an adapter failure is data, not a crash
        err = e instanceof NotPlainData ? py.head(`invalid: ${e.message}`, 500) : errorText(e);
        continue;
      }
      const problems = check(ans);
      if (!problems.length) return [ans, ''];
      err = py.head('invalid: ' + problems.slice(0, 5).map((p) => py.head(p, 200)).join('; '), 500);
    }
    return [null, err];
  }

  /** Runs the council; resolves to a plain CouncilResult object. */
  async run() {
    const a = this.adapter;
    const res = M.councilResult({
      idea: this.idea, adapter: a.name === undefined ? 'adapter' : py.str(a.name),
      synthetic: py.truthy(a.synthetic), stakeholders: [...this.roster], sources: [...this.sources],
    });
    let prior = [];
    for (const [stepId, title, question] of this.phases) {
      let it = 0;
      for (;;) {
        it += 1;
        const ctx = { idea: this.idea, iteration: it, sources: [...this.sources], prior, locked: res.locked_decisions };
        const views = [];
        for (const role of this.roster) {
          const [got, err] = await this._ask('view', checkView,
            { stakeholder: role, role, phase_id: stepId, question, context: ctx });
          if (got === null) {
            views.push(M.stakeholderView({ stakeholder: role, role, status: 'failed', error: err }));
            continue;
          }
          const raw = normalizeView(got);
          views.push(M.stakeholderView({
            stakeholder: role, role, status: 'ok', w5h1: raw.w5h1, done_so_far: raw.done_so_far,
            missing: [...raw.missing], devils_advocate: raw.devils_advocate, contribution: raw.contribution,
            sources: [...raw.sources],
          }));
          for (const fl of gates.flagUnsourced({ ...raw, stakeholder: role }, this.sources)) {
            res.flags.push({ ...fl, step_id: stepId, iteration: it });
          }
        }
        const okViews = views.filter((v) => v.status === 'ok');
        let juror;
        if (!okViews.length) {
          juror = M.jurorMerge({ synthesis_status: 'no_views', error: 'no valid view to merge' });
        } else {
          const [m, err] = await this._ask('merge', checkMerge,
            { phase_id: stepId, question, views: okViews, context: ctx });
          if (m === null) {
            juror = M.jurorMerge({ synthesis_status: 'failed', error: err });
          } else {
            juror = M.jurorMerge({
              synthesis_status: 'full', merge_summary: m.merge_summary, decision: m.decision,
              resolved: [...m.resolved], open_issues: [...m.open_issues], next_steps: [...m.next_steps],
              reversals: [...(py.has(m, 'reversals') ? m.reversals : [])], spec_delta: m.spec_delta,
              loop_recommended: m.loop_recommended,
            });
            if (!gates.synthesisOk(juror)) juror.synthesis_status = 'thin';
          }
        }
        res.rounds.push(M.councilRound({ step_id: stepId, step_title: title, iteration: it, question, views, juror }));
        res.decision_tree.push(M.decisionNode({
          step_id: stepId, iteration: it, branches: okViews.map((v) => v.contribution),
          decision: juror.decision, next_steps: [...juror.next_steps],
        }));
        if (juror.synthesis_status === 'full') {
          for (const r of juror.resolved) if (!res.locked_decisions.includes(r)) res.locked_decisions.push(r);
          prior = prior.concat([{ step: stepId, decision: juror.decision }]);
        }
        if (!juror.loop_recommended || it >= this.maxLoops) break;
      }
    }
    return res;
  }
}

const count = (map, k) => map.set(k, (map.get(k) || 0) + 1);
const sortedObj = (m) => py.fromPairs(py.sorted([...m.keys()]).map((k) => [k, m.get(k)]));

/** Counts for a run: views by status, merges by synthesis status, flags, and whether every phase's last
 * iteration reached a full synthesis (decided). */
function health(res) {
  const views = new Map();
  const merges = new Map();
  const last = new Map();
  for (const r of res.rounds) {
    for (const v of r.views) count(views, v.status);
    count(merges, r.juror.synthesis_status);
    last.set(r.step_id, r.juror.synthesis_status);
  }
  const st = [...last.values()];
  return {
    views: sortedObj(views), merges: sortedObj(merges), phases: last.size,
    phases_decided: st.filter((s) => s === 'full').length,
    decided: st.length > 0 && st.every((s) => s === 'full'),
    flags: res.flags.length, synthetic: res.synthetic,
  };
}

module.exports = { Council, DEFAULT_ROSTER, DEFAULT_PHASES, loadPhases, checkPhases, health, errorText };
