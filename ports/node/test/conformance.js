'use strict';
// Runs spec/conformance against this port, check for check as src/sf_smartsavant/conformance.py does.
// Usage: node test/conformance.js [vector dir]
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const sv = require('../src');

const dir = process.argv[2] || path.join(__dirname, '..', '..', '..', 'spec', 'conformance');
const load = (n) => JSON.parse(fs.readFileSync(path.join(dir, n), 'utf8'));

// JSON value equality: key order does not matter, true is not 1.
function same(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b || a === null || b === null || typeof a !== 'object') return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) return a.length === b.length && a.every((x, i) => same(x, b[i]));
  const ka = Object.keys(a).sort();
  const kb = Object.keys(b).sort();
  return same(ka, kb) && ka.every((k) => same(a[k], b[k]));
}
const show = (v) => { try { return sv.dumps(v).slice(0, 300); } catch (e) { return String(JSON.stringify(v)).slice(0, 300); } };

let total = 0;
const fails = [];
function check(name, got, want) {
  total++;
  if (!same(got, want)) fails.push(`${name}: got ${show(got)}, want ${show(want)}`);
}

async function main() {
  for (const c of load('canonical.json').cases) {
    const t = sv.dumps(c.input);
    check('canonical/' + c.name, [t, crypto.createHash('sha256').update(t, 'utf8').digest('hex')], [c.canonical, c.sha256]);
  }
  load('figures.json').cases.forEach((c, i) => check(`figures/${i}`, sv.figures(c.text), c.figures));
  const g = load('gates.json').cases;
  for (const c of g.flag_unsourced) check('flag_unsourced/' + c.name, sv.flagUnsourced(c.view, c.allowed), c.flags);
  const fns = { is_generic: sv.isGeneric, is_fallback: sv.isFallback, has_placeholder_code: sv.hasPlaceholderCode,
    compliance_block: (t) => sv.complianceBlock(t), squash: sv.squash };
  for (const [fn, f] of Object.entries(fns)) g[fn].forEach((c, i) => check(`${fn}/${i}`, f(c.text), c.result));
  for (const c of g.synthesis_ok) check('synthesis_ok/' + c.name, sv.synthesisOk(c.merge), c.result);
  const s = load('schema.json').cases;
  for (const c of s.view) check('schema/view/' + c.name, sv.checkView(c.input), c.errors);
  for (const c of s.merge) check('schema/merge/' + c.name, sv.checkMerge(c.input), c.errors);
  for (const c of load('runs.json').cases) {
    const res = await new sv.Council(c.idea, new sv.ScriptedAdapter(c.script), {
      roster: c.roster, phases: c.phases, sources: c.sources, maxLoops: c.max_loops, retries: c.retries,
    }).run();
    const k = c.project_key;
    const got = {
      council: () => res, digest: () => sv.runDigest(res), health: () => sv.health(res),
      stories: () => sv.toStories(res, k), spec_md: () => sv.toSpecMd(res), mermaid: () => sv.toMermaid(res),
      trainset: () => sv.toTrainset(res), trainset_all: () => sv.toTrainset(res, true), audit: () => sv.auditResult(res),
    };
    for (const field of Object.keys(c.expect)) check(`runs/${c.name}/${field}`, got[field](), c.expect[field]);
  }
  const a = load('audit.json').cases;
  for (const c of a.import) check('import/' + c.name, sv.fromSmartsignal(c.input), c.council);
  for (const c of a.council) check('audit/council/' + c.name, sv.auditResult(sv.fromSmartsignal(c.input)), c.report);
  for (const c of a.stories) check('audit/stories/' + c.name, sv.auditStories(c.input), c.report);
  for (const c of a.trainset) check('audit/trainset/' + c.name, sv.auditTrainset(c.input), c.report);

  for (const f of fails) console.log(f);
  console.log(fails.length ? `node: ${total - fails.length}/${total} vectors passed (${fails.length} failed)` : `node: ${total}/${total} vectors passed`);
  process.exit(fails.length ? 1 : 0);
}

main().catch((e) => {
  console.error(e && e.stack ? e.stack : e);
  console.log('node: FAILED');
  process.exit(1);
});
