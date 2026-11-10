'use strict';
// Canonical JSON: the byte form every SmartSavant engine hashes (run digests) and compares.
// Keys ASCII and sorted, no whitespace, integers only, and only " \ and characters below U+0020 escaped.
const crypto = require('crypto');

const SHORT = { '"': '\\"', '\\': '\\\\', '\b': '\\b', '\f': '\\f', '\n': '\\n', '\r': '\\r', '\t': '\\t' };

function str(s) {
  let out = '"';
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    const c = s.charCodeAt(i);
    if (SHORT[ch] !== undefined) out += SHORT[ch];
    else if (c < 0x20) out += '\\u' + c.toString(16).padStart(4, '0');
    else out += ch;
  }
  return out + '"';
}

/** The canonical JSON text of value. Throws TypeError for non-integer numbers and other types. */
function dumps(value) {
  if (value === null) return 'null';
  if (value === true) return 'true';
  if (value === false) return 'false';
  if (typeof value === 'number') {
    if (!Number.isInteger(value)) throw new TypeError('canonical JSON does not encode float (floats are not allowed)');
    return BigInt(value).toString();
  }
  if (typeof value === 'bigint') return value.toString();
  if (typeof value === 'string') return str(value);
  if (Array.isArray(value)) return '[' + value.map(dumps).join(',') + ']';
  if (typeof value === 'object') {
    const keys = Object.keys(value);
    for (const k of keys) {
      if (!/^[\x00-\x7f]*$/.test(k)) throw new TypeError('canonical JSON object keys must be ASCII strings');
    }
    keys.sort();
    return '{' + keys.map((k) => str(k) + ':' + dumps(value[k])).join(',') + '}';
  }
  throw new TypeError(`canonical JSON does not encode ${typeof value} (floats are not allowed)`);
}

const encode = (value) => Buffer.from(dumps(value), 'utf8');
const sha256Hex = (data) => crypto.createHash('sha256').update(typeof data === 'string' ? Buffer.from(data, 'utf8') : data).digest('hex');

module.exports = { dumps, encode, sha256Hex };
