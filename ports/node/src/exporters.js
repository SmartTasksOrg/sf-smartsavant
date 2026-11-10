'use strict';
// Exporters: a council result -> user stories, a spec in Markdown, a fine-tune trainset, a Mermaid tree.
// Every exporter reports what the run did not decide instead of covering it up.
const canonical = require('./canonical');
const gates = require('./gates');
const py = require('./py');
const { health } = require('./engine');

/** sha256 of the canonical JSON of the whole result: the run's identity in exports. */
const runDigest = (res) => canonical.sha256Hex(canonical.encode(res));

/** The last iteration of each phase, in phase order. */
function lastRounds(res) {
  const last = new Map();
  for (const r of res.rounds) last.set(r.step_id, r);
  return [...last.values()];
}

function toStories(res, projectKey) {
  const stories = lastRounds(res).map((r, n) => {
    const j = r.juror;
    const sid = `STORY-${projectKey}-${String(n + 1).padStart(4, '0')}`;
    const steps = j.next_steps.filter((s) => !gates.isGeneric(s));
    if (j.synthesis_status === 'full' && steps.length) {
      return {
        story_id: sid, status: 'ready', epic: r.step_title, title: steps[0],
        decision: j.decision, sub_tasks: steps,
        acceptance_criteria: [`Implements the decision: ${j.decision}`, ...j.resolved.map((x) => `Holds: ${x}`),
          'A test exercises it end to end.'],
        open_issues: [...j.open_issues], phase: r.step_id, iteration: r.iteration,
      };
    }
    return {
      story_id: sid, status: 'blocked', epic: r.step_title,
      title: `Decide ${r.step_title}: the council did not reach a decision (${j.synthesis_status})`,
      decision: '', sub_tasks: [], acceptance_criteria: [],
      open_issues: [...j.open_issues, ...(j.error ? [j.error] : [])],
      phase: r.step_id, iteration: r.iteration,
    };
  });
  return {
    schema: 'smartsavant.stories/1', project_key: projectKey, run: runDigest(res),
    counts: {
      stories: stories.length, ready: stories.filter((s) => s.status === 'ready').length,
      blocked: stories.filter((s) => s.status === 'blocked').length,
    },
    stories,
  };
}

const MD_META = new Set('\\`*_{}[]()#+-.!|~');
// control and bidirectional-control characters, shown as U+FFFD
const BAD = /[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/;
const BAD_G = new RegExp(BAD.source, 'g');

/** Model text made inert in Markdown: one line (whitespace squashed), control and bidirectional-control
 * characters as U+FFFD, & < > as HTML entities, and every
 * Markdown metacharacter \\ ` * _ { } [ ] ( ) # + - . ! | ~ backslash-escaped. */
function md(text) {
  let out = '';
  for (const c of gates.squash(py.str(text))) {
    if (BAD.test(c)) out += '\ufffd';
    else if (c === '&') out += '&amp;';
    else if (c === '<') out += '&lt;';
    else if (c === '>') out += '&gt;';
    else if (MD_META.has(c)) out += '\\' + c;
    else out += c;
  }
  return out;
}

function toSpecMd(res) {
  const h = health(res);
  const lines = ['# Council spec', '', `*${md(res.idea)}*`, '',
    `Run ${runDigest(res).slice(0, 16)} with adapter ${md(res.adapter)}`
    + `${res.synthetic ? ' (synthetic: not a real deliberation)' : ''}. `
    + `${h.phases_decided} of ${h.phases} phases decided; views ${py.repr(h.views)}; merges ${py.repr(h.merges)}; `
    + `${h.flags} flag(s).`, ''];
  if (!h.decided) {
    lines.push('> **Not build-ready:** at least one phase has no full synthesis. Do not build from the '
      + 'undecided phases; re-run them with a working adapter.', '');
  }
  if (res.locked_decisions.length) {
    lines.push('## Decisions', '', '| # | Decision |', '|---|---|');
    res.locked_decisions.forEach((d, i) => lines.push(`| ${i + 1} | ${md(d)} |`));
    lines.push('');
  }
  for (const r of lastRounds(res)) {
    const j = r.juror;
    lines.push(`## ${md(r.step_title)}`, '', `**Question:** ${md(r.question)}`, '');
    if (j.synthesis_status === 'full') {
      lines.push(`**Decision:** ${md(j.decision)}`, '', md(j.merge_summary), '');
      if (j.next_steps.length) lines.push('**Next steps:**', '', ...j.next_steps.map((s) => `- ${md(s)}`), '');
    } else {
      lines.push(`**NOT DECIDED** (${j.synthesis_status})${j.error ? ': ' + md(j.error) : ''}`, '');
    }
    if (j.open_issues.length) lines.push('**Open issues:**', '', ...j.open_issues.map((s) => `- ${md(s)}`), '');
    const failed = r.views.filter((v) => v.status !== 'ok').map((v) => v.stakeholder);
    if (failed.length) lines.push(`*No valid view from: ${md(failed.join(', '))}.*`, '');
  }
  if (res.flags.length) {
    lines.push('## Flags', '', `${res.flags.length} finding(s) to review before use:`, '');
    for (const f of res.flags.slice(0, 200)) {
      const fig = py.get(f, 'figure');
      const src = py.get(f, 'source');
      const what = py.truthy(fig) ? fig : py.truthy(src) ? src : '';
      lines.push(`- ${py.str(f.flag)}: ${md(what)} (${md(py.get(f, 'stakeholder', ''))}, ${md(py.get(f, 'step_id', ''))})`);
    }
    lines.push('');
  }
  return lines.join('\n');
}

const METHODOLOGY = {
  frame: 'Establish the problem, the real users and their jobs-to-be-done, and agree on one long-term goal.',
  concept: 'Collect solution directions, then pick the wedge: the smallest offer the users would switch for.',
  design: 'Draw the user flow and the data flow end to end, and name every screen and every store.',
  build: 'Decide the technical approach and the riskiest part to prove first.',
  integrate: 'Fit the design into the target architecture: owners, interfaces, auth, deployment.',
  measure: 'Set the KPIs, how each one is measured, and the thresholds that would change the plan.',
  spec: 'Assemble the decisions into one document that an engineer can build from.',
};

const VIEW_FIELDS = ['w5h1', 'done_so_far', 'missing', 'devils_advocate', 'contribution', 'sources'];

/** Rows {instruction, input, output, meta}: methodology per phase used; expert rows (ok views without a
 * flag) and facilitation rows (full merges) unless the run is synthetic and allowSynthetic is false. */
function toTrainset(res, allowSynthetic = false) {
  const digest = runDigest(res);
  const rows = [];
  const flagged = new Set(res.flags.map((f) => JSON.stringify([py.get(f, 'step_id', null), py.get(f, 'iteration', null), py.get(f, 'stakeholder', null)])));
  const learn = allowSynthetic || !res.synthetic;
  const seen = new Set();
  for (const r of res.rounds) {
    const meta = { phase: r.step_id, iteration: r.iteration, run: digest, adapter: res.adapter };
    if (!seen.has(r.step_id) && py.has(METHODOLOGY, r.step_id) && r.step_title.toLowerCase().startsWith(r.step_id)) {
      seen.add(r.step_id);
      rows.push({
        instruction: `We are at the ${r.step_title} phase of a design council. How should I facilitate it?`,
        input: '', output: METHODOLOGY[r.step_id], meta: { ...meta, kind: 'methodology' },
      });
    }
    if (!learn) continue;
    for (const v of r.views) {
      if (v.status === 'ok' && !flagged.has(JSON.stringify([r.step_id, r.iteration, v.stakeholder]))) {
        rows.push({
          instruction: `As the ${v.stakeholder} on a design council, give your view on: ${r.question}`,
          input: res.idea,
          output: py.jsonDumps(Object.fromEntries(VIEW_FIELDS.map((k) => [k, v[k]])), true),
          meta: { ...meta, kind: 'expert', stakeholder: v.stakeholder },
        });
      }
    }
    if (r.juror.synthesis_status === 'full') {
      const j = r.juror;
      rows.push({
        instruction: `Merge the panel for the ${r.step_title} phase as the juror.`,
        input: py.jsonDumps(r.views.filter((v) => v.status === 'ok').map((v) => v.contribution)),
        output: py.jsonDumps({ decision: j.decision, merge_summary: j.merge_summary, resolved: j.resolved, next_steps: j.next_steps }, true),
        meta: { ...meta, kind: 'facilitation', synthesis: 'full' },
      });
    }
  }
  return rows;
}

/** A Mermaid label: squashed, control characters as U+FFFD, quotes as apostrophes, < > & as entities, at most 80 code points, quoted. */
function label(t) {
  const s = py.head(gates.squash(py.str(t)).replace(BAD_G, '\ufffd'), 80);
  return '"' + s.replace(/&/g, '&amp;').replace(/"/g, "'").replace(/</g, '&lt;').replace(/>/g, '&gt;') + '"';
}

function toMermaid(res) {
  const q = label;
  const lines = ['flowchart TD', '  start([Idea])'];
  let prev = 'start';
  res.decision_tree.forEach((n, i) => {
    const nid = `n${i}`;
    const r = res.rounds[i];
    const label = r.juror.synthesis_status === 'full' ? n.decision : `NOT DECIDED: ${r.juror.synthesis_status}`;
    lines.push(`  ${nid}{${q(r.step_title + ' #' + py.str(n.iteration))}}`);
    lines.push(`  ${prev} --> ${nid}`);
    lines.push(`  ${nid}d[${q(label)}]`);
    lines.push(`  ${nid} --> ${nid}d`);
    prev = nid;
  });
  return lines.join('\n') + '\n';
}

module.exports = { runDigest, toStories, toSpecMd, toTrainset, toMermaid, METHODOLOGY, md };
