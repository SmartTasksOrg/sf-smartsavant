'use strict';
// The council data model as plain objects, with fields in the order of the Python dataclasses
// (src/sf_smartsavant/model.py and bindings/council.d.ts).
//   view.status              ok | failed | offline
//   juror.synthesis_status   full | thin | recovered | failed | offline | no_views

const VIEW_STATUSES = ['ok', 'failed', 'offline'];
const SYNTHESIS_STATUSES = ['full', 'thin', 'recovered', 'failed', 'offline', 'no_views'];

function w5h1(f = {}) {
  return { who: f.who ?? '', what: f.what ?? '', where: f.where ?? '', when: f.when ?? '', why: f.why ?? '', how: f.how ?? '' };
}

function stakeholderView(f) {
  return {
    stakeholder: f.stakeholder, role: f.role, status: f.status ?? 'ok', w5h1: w5h1(f.w5h1),
    done_so_far: f.done_so_far ?? '', missing: f.missing ?? [], devils_advocate: f.devils_advocate ?? '',
    contribution: f.contribution ?? '', sources: f.sources ?? [], error: f.error ?? '',
  };
}

function jurorMerge(f = {}) {
  return {
    synthesis_status: f.synthesis_status ?? 'full', merge_summary: f.merge_summary ?? '', decision: f.decision ?? '',
    resolved: f.resolved ?? [], open_issues: f.open_issues ?? [], next_steps: f.next_steps ?? [],
    reversals: f.reversals ?? [], spec_delta: f.spec_delta ?? '', loop_recommended: f.loop_recommended ?? false,
    error: f.error ?? '',
  };
}

function councilRound(f) {
  return {
    step_id: f.step_id, step_title: f.step_title, iteration: f.iteration, question: f.question,
    views: f.views ?? [], juror: f.juror ?? jurorMerge(),
  };
}

function decisionNode(f) {
  return { step_id: f.step_id, iteration: f.iteration, branches: f.branches ?? [], decision: f.decision ?? '', next_steps: f.next_steps ?? [] };
}

function councilResult(f = {}) {
  return {
    schema: f.schema ?? 'smartsavant.council/1', idea: f.idea ?? '', adapter: f.adapter ?? '',
    synthetic: f.synthetic ?? false, stakeholders: f.stakeholders ?? [], sources: f.sources ?? [],
    rounds: f.rounds ?? [], decision_tree: f.decision_tree ?? [], locked_decisions: f.locked_decisions ?? [],
    flags: f.flags ?? [],
  };
}

/** A council.json object (validate it with checkCouncil first) rebuilt with every default filled in. */
function resultFromDict(d) {
  const rounds = (d.rounds || []).map((r) => councilRound({
    ...r, views: (r.views || []).map(stakeholderView), juror: jurorMerge(r.juror || {}),
  }));
  return councilResult({ ...d, rounds, decision_tree: (d.decision_tree || []).map(decisionNode) });
}

module.exports = {
  VIEW_STATUSES, SYNTHESIS_STATUSES, w5h1, stakeholderView, jurorMerge, councilRound, decisionNode, councilResult,
  resultFromDict,
};
