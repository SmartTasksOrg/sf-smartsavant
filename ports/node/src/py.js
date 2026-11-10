'use strict';
// Python behaviour the reference relies on: code-point lengths and slices, str.strip(chars), str(), repr(),
// truthiness, dict.get() and json.dumps() (default separators, ensure_ascii=False).

const has = (o, k) => o !== null && typeof o === 'object' && Object.prototype.hasOwnProperty.call(o, k);

/** dict.get(k, d): a present key with a null value gives null, not the default. */
const get = (o, k, d) => (has(o, k) ? o[k] : d);

const isDict = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const isStr = (v) => typeof v === 'string';
const isInt = (v) => typeof v === 'number' && Number.isInteger(v);

/** len(s) in code points. */
function len(s) {
  let n = 0;
  for (const _ of s) n++; // eslint-disable-line no-unused-vars
  return n;
}

/** s[:n] in code points. */
function head(s, n) {
  if (s.length <= n) return s;
  let out = '';
  let i = 0;
  for (const ch of s) {
    if (i++ >= n) break;
    out += ch;
  }
  return out;
}

/** str.strip(chars). */
function stripChars(s, chars) {
  const set = new Set(chars);
  const cps = Array.from(s);
  let a = 0;
  let b = cps.length;
  while (a < b && set.has(cps[a])) a++;
  while (b > a && set.has(cps[b - 1])) b--;
  return cps.slice(a, b).join('');
}

/** Python sort order of strings: by code point (JS compares UTF-16 units). */
function cmpStr(a, b) {
  const x = Array.from(a);
  const y = Array.from(b);
  const n = Math.min(x.length, y.length);
  for (let i = 0; i < n; i++) {
    const d = x[i].codePointAt(0) - y[i].codePointAt(0);
    if (d) return d;
  }
  return x.length - y.length;
}
const sorted = (arr) => [...arr].sort(cmpStr);

/** repr(float) / repr(int). JSON.parse cannot tell 1.0 from 1, so an integral number prints as an int. */
function num(n) {
  if (Number.isInteger(n)) return BigInt(n).toString();
  if (Number.isNaN(n)) return 'nan';
  if (!Number.isFinite(n)) return n > 0 ? 'inf' : '-inf';
  const [mant, e] = n.toExponential().split('e');
  const exp = Number(e);
  if (exp >= -4 && exp < 16) {
    const s = String(n); // no exponent in this range
    return s.includes('.') ? s : s + '.0';
  }
  return mant + 'e' + (exp < 0 ? '-' : '+') + String(Math.abs(exp)).padStart(2, '0');
}

// str.isprintable() is false for these (and ' ' is printable); Node's Unicode tables may differ from
// Python's for characters assigned in recent Unicode versions.
const UNPRINTABLE = /^[\p{Cc}\p{Cf}\p{Cs}\p{Co}\p{Cn}\p{Zl}\p{Zp}\p{Zs}]$/u;

/** repr(str): quotes as Python picks them; non-printable characters escaped as \xNN, \uNNNN, \UNNNNNNNN. */
function reprStr(s) {
  const q = s.includes("'") && !s.includes('"') ? '"' : "'";
  let out = q;
  for (const ch of s) {
    const c = ch.codePointAt(0);
    if (ch === q || ch === '\\') out += '\\' + ch;
    else if (ch === '\n') out += '\\n';
    else if (ch === '\r') out += '\\r';
    else if (ch === '\t') out += '\\t';
    else if (c !== 0x20 && UNPRINTABLE.test(ch)) {
      out += c < 0x100 ? '\\x' + c.toString(16).padStart(2, '0')
        : c < 0x10000 ? '\\u' + c.toString(16).padStart(4, '0') : '\\U' + c.toString(16).padStart(8, '0');
    }
    else out += ch;
  }
  return out + q;
}

function repr(v) {
  if (v === null || v === undefined) return 'None';
  if (v === true) return 'True';
  if (v === false) return 'False';
  if (typeof v === 'number') return num(v);
  if (typeof v === 'string') return reprStr(v);
  if (Array.isArray(v)) return '[' + v.map(repr).join(', ') + ']';
  if (typeof v === 'object') return '{' + Object.keys(v).map((k) => reprStr(k) + ': ' + repr(v[k])).join(', ') + '}';
  return String(v);
}

/** str(v). */
const str = (v) => (typeof v === 'string' ? v : repr(v));

/** bool(v). */
function truthy(v) {
  if (v === null || v === undefined || v === false || v === 0 || v === '') return false;
  if (Array.isArray(v)) return v.length > 0;
  if (typeof v === 'object') return Object.keys(v).length > 0;
  return true;
}

function jsonStr(s, ensureAscii = false) {
  let out = '"';
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    const c = s.charCodeAt(i);
    if (ch === '"') out += '\\"';
    else if (ch === '\\') out += '\\\\';
    else if (ch === '\n') out += '\\n';
    else if (ch === '\r') out += '\\r';
    else if (ch === '\t') out += '\\t';
    else if (ch === '\b') out += '\\b';
    else if (ch === '\f') out += '\\f';
    else if (c < 0x20 || (ensureAscii && c > 0x7e)) out += '\\u' + c.toString(16).padStart(4, '0');
    else out += ch;
  }
  return out + '"';
}

/** json.dumps(v, ensure_ascii=False[, sort_keys=True]) with the default ', ' and ': ' separators;
 * ensureAscii writes non-ASCII as \\uXXXX (surrogate pairs for astral characters), as Python's default. */
function jsonDumps(v, sortKeys = false, ensureAscii = false) {
  if (v === null || v === undefined) return 'null';
  if (v === true) return 'true';
  if (v === false) return 'false';
  if (typeof v === 'number') {
    if (Number.isNaN(v)) return 'NaN';
    if (!Number.isFinite(v)) return v > 0 ? 'Infinity' : '-Infinity';
    return num(v);
  }
  if (typeof v === 'string') return jsonStr(v, ensureAscii);
  if (Array.isArray(v)) return '[' + v.map((x) => jsonDumps(x, sortKeys, ensureAscii)).join(', ') + ']';
  if (typeof v === 'object') {
    const keys = sortKeys ? sorted(Object.keys(v)) : Object.keys(v);
    return '{' + keys.map((k) => jsonStr(k, ensureAscii) + ': ' + jsonDumps(v[k], sortKeys, ensureAscii)).join(', ') + '}';
  }
  throw new TypeError(`Object of type ${typeof v} is not JSON serializable`);
}

/** o[k] = v as an own data property, also for k === '__proto__' (plain assignment would set the prototype). */
function setKey(o, k, v) {
  Object.defineProperty(o, k, { value: v, enumerable: true, writable: true, configurable: true });
  return o;
}

/** Deep copy of JSON-like data into plain objects and arrays (copy.deepcopy for what adapters see).
 * Use it on data this package built or already checked with schema.plain; it does not validate. */
function clone(v) {
  if (Array.isArray(v)) return v.map(clone);
  if (v !== null && typeof v === 'object') {
    const out = {};
    for (const k of Object.keys(v)) setKey(out, k, clone(v[k]));
    return out;
  }
  return v;
}

/** An object from [key, value] pairs, '__proto__' kept as data. */
const fromPairs = (pairs) => pairs.reduce((o, [k, v]) => setKey(o, k, v), {});

// a UTF-16 surrogate without its partner: what Python sees as a lone surrogate code point
const LONE = /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/;
const LONE_G = new RegExp(LONE.source, 'g');
const hasLone = (s) => LONE.test(s);
const replaceLone = (s, by) => s.replace(LONE_G, by);

module.exports = {
  has, get, isDict, isStr, isInt, len, head, stripChars, cmpStr, sorted, num, repr, str, truthy,
  jsonDumps, clone, setKey, fromPairs, hasLone, replaceLone,
};
