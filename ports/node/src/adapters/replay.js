'use strict';
// Replays the answers of a recorded run (a council.json written by an engine of this package).
// Views and merges whose status was not ok / full / thin are replayed as failures (the adapter throws
// ReplayError), so the replayed run has the same health as the original.
const fs = require('fs');
const py = require('../py');
const { LLMAdapter } = require('./base');

class ReplayError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ReplayError';
  }
}

const k3 = (...parts) => JSON.stringify(parts);

class ReplayAdapter extends LLMAdapter {
  /** recorded: a council object, or the path of a council.json file. */
  constructor(recorded) {
    super();
    this.name = 'replay';
    const doc = typeof recorded === 'string' ? JSON.parse(fs.readFileSync(recorded, 'utf8')) : recorded;
    this.synthetic = py.truthy(py.get(doc, 'synthetic', false));
    this._views = new Map();
    this._merges = new Map();
    for (const r of py.get(doc, 'rounds', [])) {
      for (const v of py.get(r, 'views', [])) {
        const key = k3(r.step_id, r.iteration, v.stakeholder);
        if (!this._views.has(key)) this._views.set(key, v);
      }
      this._merges.set(k3(r.step_id, r.iteration), py.get(r, 'juror', {}));
    }
  }

  view({ stakeholder, phase_id, context }) {
    const v = this._views.get(k3(phase_id, py.get(context, 'iteration', 1), stakeholder));
    if (v === undefined || py.get(v, 'status') !== 'ok') {
      throw new ReplayError(`no recorded valid view for ${stakeholder} at ${phase_id}`);
    }
    return py.clone({
      w5h1: v.w5h1, done_so_far: v.done_so_far, missing: v.missing, devils_advocate: v.devils_advocate,
      contribution: v.contribution, sources: v.sources,
    });
  }

  merge({ phase_id, context }) {
    const m = this._merges.get(k3(phase_id, py.get(context, 'iteration', 1)));
    if (m === undefined || !['full', 'thin'].includes(py.get(m, 'synthesis_status'))) {
      throw new ReplayError(`no recorded valid merge for ${phase_id}`);
    }
    const out = {};
    for (const k of ['merge_summary', 'decision', 'resolved', 'open_issues', 'next_steps', 'reversals', 'spec_delta',
      'loop_recommended']) out[k] = m[k];
    return py.clone(out);
  }
}

module.exports = { ReplayAdapter, ReplayError };
