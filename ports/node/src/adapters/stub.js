'use strict';
// A deterministic adapter: no model, no network. For tests, demos and ports (the vectors pin its output).
// It is synthetic: its views are templates, so nothing it produces is a real decision.
//   tag  = first 8 hex digits of sha256(UTF-8(stakeholder + "|" + phase_id + "|" + idea)), written as
//          "sv" + tag so that it never reads as a figure ("4007743b" would be 4007743 billion)
//   why  = the first 120 code points of the idea
const crypto = require('crypto');
const py = require('../py');
const { LLMAdapter } = require('./base');

const tag = (...parts) => crypto.createHash('sha256').update(Buffer.from(parts.join('|'), 'utf8')).digest('hex').slice(0, 8);

class StubAdapter extends LLMAdapter {
  constructor() {
    super();
    this.name = 'stub';
    this.synthetic = true;
  }

  view({ stakeholder, role, phase_id, context }) {
    const idea = py.get(context, 'idea', '');
    const t = tag(stakeholder, phase_id, idea);
    return {
      w5h1: {
        who: stakeholder, what: `advance ${phase_id}`, where: 'the target architecture',
        when: 'this iteration', why: py.head(idea, 120), how: `deterministic stub sv${t}`,
      },
      done_so_far: `${role} framed ${phase_id}.`,
      missing: [`${phase_id}: target files and modules`],
      devils_advocate: `Risk at ${phase_id}: an unproven assumption (sv${t}).`,
      contribution: `${role} proposes the ${phase_id} step sv${t}.`,
      sources: [],
    };
  }

  merge({ phase_id, views }) {
    const resolved = views.map((v, i) => `${phase_id} ${i + 1}: ${v.contribution}`).slice(0, 5);
    // dict.fromkeys: duplicates removed, first occurrence wins
    const issues = [...new Set(views.flatMap((v) => py.get(v, 'missing', [])))].slice(0, 5);
    return {
      merge_summary: `Stub juror merged ${views.length} view(s) for ${phase_id}.`,
      decision: `${phase_id}: proceed with the stub plan`,
      resolved,
      open_issues: issues,
      next_steps: [`Write the ${phase_id} module for the stub plan`],
      reversals: [],
      spec_delta: `[${phase_id}] stub decision recorded.`,
      loop_recommended: false,
    };
  }
}

module.exports = { StubAdapter };
