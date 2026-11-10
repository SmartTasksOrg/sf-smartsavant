'use strict';
// Validation of what an adapter returns, and of council files. The error strings and their order are
// part of the contract (spec/conformance/schema.json).
const py = require('./py');
const { blank } = require('./gates');

const MAX_STR = 8000;
const MAX_DELTA = 20000;
const MAX_ITEMS = 20;
const MAX_ITEM = 2000;
const SOURCE_ID = /^[A-Za-z0-9._:/#@-]{1,200}$/;
const W5H1_KEYS = ['who', 'what', 'where', 'when', 'why', 'how'];
const VIEW_KEYS = ['w5h1', 'done_so_far', 'missing', 'devils_advocate', 'contribution', 'sources'];
const MERGE_KEYS = ['merge_summary', 'decision', 'resolved', 'open_issues', 'next_steps', 'reversals', 'spec_delta',
  'loop_recommended'];
const MERGE_REQUIRED = MERGE_KEYS.filter((k) => k !== 'reversals');
const MAX_DEPTH = 32;
const MAX_TOTAL = 1000000; // code points of all strings in one answer, keys included

class NotPlainData extends Error {
  constructor(message) {
    super(message);
    this.name = 'NotPlainData';
  }
}

function typeName(x) {
  if (x === undefined) return 'undefined';
  if (typeof x !== 'object' && typeof x !== 'function') return typeof x;
  try {
    const p = Object.getPrototypeOf(x);
    const c = p && Object.prototype.hasOwnProperty.call(p, 'constructor') ? p.constructor : null;
    return typeof c === 'function' && typeof c.name === 'string' && c.name ? c.name : typeof x;
  } catch (e) {
    return typeof x;
  }
}

/**
 * A copy of an adapter answer made only of null, booleans, numbers, strings, arrays and plain objects,
 * built before anything else so that what is validated is exactly what is used (schema.plain in Python).
 * Refused with NotPlainData: other types (Map, Date, undefined, functions, BigInt ...), subclasses of Array
 * or Object, getters and setters, symbol keys, holes in arrays, nesting deeper than 32 and more than
 * 1,000,000 code points of text. Every property is read once, through its descriptor, so a getter or a
 * Proxy cannot give the check one value and the engine another. A '__proto__' key stays a data key.
 */
function plain(obj) {
  let budget = MAX_TOTAL;
  const own = (x, k) => {
    const d = Object.getOwnPropertyDescriptor(x, k);
    if (d === undefined) throw new NotPlainData('undefined is not JSON data');
    if (d.get !== undefined || d.set !== undefined || !('value' in d)) throw new NotPlainData('getters are not JSON data');
    return d.value;
  };
  const go = (x, depth) => {
    if (depth > MAX_DEPTH) throw new NotPlainData('nested deeper than 32 levels');
    if (x === null || typeof x === 'boolean' || typeof x === 'number') return x;
    if (typeof x === 'string') {
      budget -= py.len(x);
      if (budget < 0) throw new NotPlainData('more than 1,000,000 characters of text');
      return x;
    }
    if (typeof x !== 'object') throw new NotPlainData(`${typeName(x)} is not JSON data`);
    const proto = Object.getPrototypeOf(x);
    if (Array.isArray(x)) {
      if (proto !== Array.prototype) throw new NotPlainData(`${typeName(x)} is not JSON data`);
      const n = own(x, 'length');
      const out = [];
      for (let i = 0; i < n; i++) out.push(go(own(x, String(i)), depth + 1));
      return out;
    }
    if (proto !== Object.prototype && proto !== null) throw new NotPlainData(`${typeName(x)} is not JSON data`);
    const out = {};
    for (const k of Reflect.ownKeys(x)) {
      if (typeof k !== 'string') throw new NotPlainData('object keys must be strings');
      budget -= py.len(k);
      py.setKey(out, k, go(own(x, k), depth + 1));
    }
    if (budget < 0) throw new NotPlainData('more than 1,000,000 characters of text');
    return out;
  };
  return go(obj, 0);
}

/** An unknown key as shown in an error: control characters (U+0000-U+001F, U+007F-U+009F) dropped,
 * unpaired surrogates as U+FFFD, then at most 40 code points. Surrogates are replaced per code point, so
 * a lone high and a lone low surrogate made adjacent by a dropped control stay two U+FFFD, as in Python. */
function showKey(k) {
  const s = Array.from(py.str(k)).filter((c) => {
    const n = c.codePointAt(0);
    return !(n < 32 || (n >= 127 && n < 160));
  }).map((c) => (c.length === 1 && c >= '\ud800' && c <= '\udfff' ? '\ufffd' : c));
  return s.slice(0, 40).join('') + (s.length > 40 ? '...' : '');
}

function checkStr(errs, where, v, limit = MAX_STR, nonempty = false) {
  if (typeof v !== 'string') errs.push(`${where} must be a string`);
  else if (py.len(v) > limit) errs.push(`${where} is longer than ${limit} characters`);
  else if (v.includes('\x00')) errs.push(`${where} contains a NUL character`);
  else if (py.hasLone(v)) errs.push(`${where} contains an unpaired surrogate`);
  else if (nonempty && blank(v)) errs.push(`${where} must not be empty`);
}

function checkList(errs, where, v, pattern = null) {
  if (!Array.isArray(v)) {
    errs.push(`${where} must be a list of strings`);
    return;
  }
  if (v.length > MAX_ITEMS) errs.push(`${where} has more than ${MAX_ITEMS} items`);
  v.slice(0, MAX_ITEMS + 1).forEach((x, i) => {
    if (blank(x)) errs.push(`${where}[${i}] must be a non-empty string`);
    else if (py.len(x) > MAX_ITEM) errs.push(`${where}[${i}] is longer than ${MAX_ITEM} characters`);
    else if (pattern !== null && !pattern.test(x)) errs.push(`${where}[${i}] is not a valid source id`);
    else if (x.includes('\x00')) errs.push(`${where}[${i}] contains a NUL character`);
    else if (py.hasLone(x)) errs.push(`${where}[${i}] contains an unpaired surrogate`);
  });
}

const extraKeys = (o, allowed) => py.sorted(Object.keys(o).filter((k) => !allowed.includes(k)));
const missingKeys = (o, required) => py.sorted(required.filter((k) => !py.has(o, k)));

/** [] when v is a valid view; otherwise the reasons, first problem first. */
function checkView(v) {
  const errs = [];
  if (!py.isDict(v)) return ['a view must be a JSON object'];
  const extra = extraKeys(v, VIEW_KEYS);
  if (extra.length) errs.push(`unknown keys (${extra.length}): ` + extra.slice(0, 5).map(showKey).join(', '));
  const missing = missingKeys(v, VIEW_KEYS);
  if (missing.length) {
    errs.push('missing keys: ' + missing.join(', '));
    return errs;
  }
  const w = v.w5h1;
  const wk = py.isDict(w) ? Object.keys(w) : null;
  if (!wk || wk.length !== W5H1_KEYS.length || !W5H1_KEYS.every((k) => py.has(w, k))) {
    errs.push('w5h1 must be an object with exactly who, what, where, when, why, how');
  } else {
    for (const k of W5H1_KEYS) checkStr(errs, `w5h1.${k}`, w[k]);
  }
  checkStr(errs, 'done_so_far', v.done_so_far);
  if (typeof v.missing === 'string') checkStr(errs, 'missing', v.missing);
  else checkList(errs, 'missing', v.missing);
  checkStr(errs, 'devils_advocate', v.devils_advocate, MAX_STR, true);
  checkStr(errs, 'contribution', v.contribution);
  checkList(errs, 'sources', v.sources, SOURCE_ID);
  return errs;
}

/** [] when m is a valid merge; otherwise the reasons, first problem first. */
function checkMerge(m) {
  const errs = [];
  if (!py.isDict(m)) return ['a merge must be a JSON object'];
  const extra = extraKeys(m, MERGE_KEYS);
  if (extra.length) errs.push(`unknown keys (${extra.length}): ` + extra.slice(0, 5).map(showKey).join(', '));
  const missing = missingKeys(m, MERGE_REQUIRED);
  if (missing.length) {
    errs.push('missing keys: ' + missing.join(', '));
    return errs;
  }
  checkStr(errs, 'merge_summary', m.merge_summary, MAX_STR, true);
  checkStr(errs, 'decision', m.decision);
  for (const k of ['resolved', 'open_issues', 'next_steps']) checkList(errs, k, m[k]);
  if (py.has(m, 'reversals')) checkList(errs, 'reversals', m.reversals);
  checkStr(errs, 'spec_delta', m.spec_delta, MAX_DELTA);
  if (typeof m.loop_recommended !== 'boolean') errs.push('loop_recommended must be true or false');
  return errs;
}

/** A valid view with missing always a list (a shallow copy). */
function normalizeView(v) {
  const out = { ...v };
  if (typeof out.missing === 'string') out.missing = blank(out.missing) ? [] : [out.missing];
  return out;
}

// ---------------------------------------------------------------- council files
const STATUS_V = ['ok', 'failed', 'offline'];
const STATUS_J = ['full', 'thin', 'recovered', 'failed', 'offline', 'no_views'];
const TOP = ['schema', 'idea', 'adapter', 'synthetic', 'stakeholders', 'sources', 'rounds', 'decision_tree',
  'locked_decisions', 'flags'];
const VIEW_REC = { stakeholder: 's', role: 's', status: 's', w5h1: 'w', done_so_far: 's', missing: 'l',
  devils_advocate: 's', contribution: 's', sources: 'l', error: 's' };
const JUROR_REC = { synthesis_status: 's', merge_summary: 's', decision: 's', resolved: 'l', open_issues: 'l',
  next_steps: 'l', reversals: 'l', spec_delta: 's', loop_recommended: 'b', error: 's' };
const ROUND_REC = { step_id: 's', step_title: 's', iteration: 'i', question: 's', views: 'L', juror: 'J' };
const NODE_REC = { step_id: 's', iteration: 'i', branches: 'l', decision: 's', next_steps: 'l' };

const sameKeys = (o, keys) => Object.keys(o).length === keys.length && keys.every((k) => py.has(o, k));
const goodStr = (x) => typeof x === 'string' && !py.hasLone(x);

const TYPE_OK = {
  s: goodStr,
  i: (v) => py.isInt(v) && v >= 1,
  b: (v) => typeof v === 'boolean',
  l: (v) => Array.isArray(v) && v.every(goodStr),
  w: (v) => py.isDict(v) && sameKeys(v, W5H1_KEYS) && Object.values(v).every(goodStr),
  L: (v) => Array.isArray(v),
  J: (v) => py.isDict(v),
};

function typed(errs, where, d, spec) {
  const keys = Object.keys(spec);
  if (!py.isDict(d) || !sameKeys(d, keys)) {
    errs.push(`${where} must have exactly the keys ` + keys.join(', '));
    return;
  }
  for (const k of keys) if (!TYPE_OK[spec[k]](d[k])) errs.push(`${where}.${k} has the wrong type`);
}

/** A council.json written by this package: exact keys and JSON types everywhere, known statuses, no
 * unpaired surrogates. At most 50 errors. */
function checkCouncil(d) {
  const errs = [];
  if (!py.isDict(d)) return ['a council file must be a JSON object'];
  if (py.get(d, 'schema') !== 'smartsavant.council/1') return ['schema must be "smartsavant.council/1"'];
  if (!sameKeys(d, TOP)) return ['top-level keys must be exactly ' + py.sorted(TOP).join(', ')];
  for (const k of ['idea', 'adapter']) if (!goodStr(d[k])) errs.push(`${k} must be a string`);
  if (typeof d.synthetic !== 'boolean') errs.push('synthetic must be true or false');
  for (const k of ['stakeholders', 'sources', 'locked_decisions']) {
    if (!TYPE_OK.l(d[k])) errs.push(`${k} must be a list of strings`);
  }
  if (!Array.isArray(d.rounds) || !Array.isArray(d.decision_tree) || !Array.isArray(d.flags)) {
    return errs.concat(['rounds, decision_tree and flags must be lists']);
  }
  if (d.decision_tree.length !== d.rounds.length) errs.push('decision_tree must have one node per round');
  d.rounds.forEach((r, i) => {
    const w = `rounds[${i}]`;
    typed(errs, w, r, ROUND_REC);
    if (!py.isDict(r) || !Array.isArray(py.get(r, 'views')) || !py.isDict(py.get(r, 'juror'))) return;
    r.views.forEach((v, j) => {
      typed(errs, `${w}.views[${j}]`, v, VIEW_REC);
      if (py.isDict(v) && !STATUS_V.includes(py.get(v, 'status'))) {
        errs.push(`${w}.views[${j}].status must be one of ${STATUS_V.join(', ')}`);
      }
    });
    typed(errs, `${w}.juror`, r.juror, JUROR_REC);
    if (!STATUS_J.includes(py.get(r.juror, 'synthesis_status'))) {
      errs.push(`${w}.juror.synthesis_status must be one of ${STATUS_J.join(', ')}`);
    }
  });
  d.decision_tree.forEach((n, i) => typed(errs, `decision_tree[${i}]`, n, NODE_REC));
  d.flags.forEach((f, i) => {
    if (!py.isDict(f) || !Object.values(f).every((x) => goodStr(x) || py.isInt(x))) {
      errs.push(`flags[${i}] must be an object of strings and integers`);
    }
  });
  return errs.slice(0, 50);
}

module.exports = {
  MAX_STR, MAX_DELTA, MAX_ITEMS, MAX_ITEM, MAX_DEPTH, MAX_TOTAL, SOURCE_ID, W5H1_KEYS, VIEW_KEYS, MERGE_KEYS,
  NotPlainData, plain, checkView, checkMerge, normalizeView, checkCouncil,
};
