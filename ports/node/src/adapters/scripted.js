'use strict';
// An adapter that answers from a script: for tests, the conformance vectors and ports.
//   new ScriptedAdapter({
//     views:  {"frame|1|Developer": [ANSWER, ...]},     // one entry per call, in order
//     merges: {"build|1": [{raise: "timeout"}, ANSWER]},
//   })
// A key not in the script is answered by the stub. An entry {raise: MESSAGE} throws AdapterError(MESSAGE);
// anything else is returned as the answer (valid or not: the engine checks). After the list for a key is
// used up, its last entry repeats. synthetic is true.
const py = require('../py');
const { StubAdapter } = require('./stub');

class AdapterError extends Error {
  constructor(message) {
    super(message);
    this.name = 'AdapterError';
  }
}

class ScriptedAdapter extends StubAdapter {
  constructor(script) {
    super();
    this.name = 'scripted';
    this.synthetic = true;
    const s = script || {};
    this.views = { ...py.get(s, 'views', {}) };
    this.merges = { ...py.get(s, 'merges', {}) };
    this._n = new Map(); // calls per key; view and merge keys never collide (different shapes)
  }

  _next(table, key) {
    const seq = table[key];
    if (!py.truthy(seq)) return null;
    const i = this._n.get(key) || 0;
    this._n.set(key, i + 1);
    const ans = seq[Math.min(i, seq.length - 1)];
    if (py.isDict(ans) && Object.keys(ans).length === 1 && py.has(ans, 'raise')) throw new AdapterError(py.str(ans.raise));
    return py.clone(ans);
  }

  view(args) {
    const key = `${args.phase_id}|${py.str(py.get(args.context, 'iteration', 1))}|${args.stakeholder}`;
    if (py.has(this.views, key)) return this._next(this.views, key);
    return super.view(args);
  }

  merge(args) {
    const key = `${args.phase_id}|${py.str(py.get(args.context, 'iteration', 1))}`;
    if (py.has(this.merges, key)) return this._next(this.merges, key);
    return super.merge(args);
  }
}

module.exports = { AdapterError, ScriptedAdapter };
