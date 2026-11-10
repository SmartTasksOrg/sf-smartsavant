'use strict';
// The one boundary between the engine and a model. An adapter answers two calls and nothing else:
//   view({stakeholder, role, phase_id, question, context})  -> ViewAnswer  (or a Promise of it)
//   merge({phase_id, question, views, context})              -> MergeAnswer (or a Promise of it)
// context is a fresh copy for every call: {idea, iteration, sources, prior, locked}. views are the valid
// views of the phase as plain objects. The engine validates every answer, retries an invalid one and
// records a failure instead of inventing content. Set `synthetic = true` for adapters whose output is not
// a real deliberation (stubs, demos).

class LLMAdapter {
  constructor() {
    this.name = 'adapter';
    this.synthetic = false;
  }

  // eslint-disable-next-line no-unused-vars
  view(args) {
    throw new Error('view() is not implemented');
  }

  // eslint-disable-next-line no-unused-vars
  merge(args) {
    throw new Error('merge() is not implemented');
  }
}

/** True for anything with view and merge methods (an LLMAdapter subclass or a plain object). */
const isAdapter = (a) => a !== null && typeof a === 'object' && typeof a.view === 'function' && typeof a.merge === 'function';

module.exports = { LLMAdapter, isAdapter };
