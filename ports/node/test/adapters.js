'use strict';
// Adapter behaviour the vectors do not cover: async adapters, thrown values that are not Errors, and the
// shape of what merge() receives. Usage: node test/adapters.js
const assert = require('assert');
const sv = require('../src');

const view = (who, phase) => ({
  w5h1: { who, what: 'w', where: 'x', when: 'y', why: 'z', how: 'h' }, done_so_far: '', missing: 'one thing',
  devils_advocate: `risk at ${phase}`, contribution: `${who} on ${phase}`, sources: [],
});
const merge = (phase) => ({
  merge_summary: 'Merged.', decision: `${phase}: decided`, resolved: [`${phase}: resolved`], open_issues: [],
  next_steps: [`Write the ${phase} parser`], spec_delta: '', loop_recommended: false,
});
const PHASES = [['frame', 'Frame', 'What?'], ['build', 'Build', 'How?']];
const ROSTER = ['Dev', 'PO'];

async function asyncAdapter() {
  class Slow extends sv.LLMAdapter {
    constructor() {
      super();
      this.name = 'slow';
      this.calls = [];
    }
    async view({ stakeholder, phase_id }) {
      await new Promise((r) => setTimeout(r, 5));
      this.calls.push(`${phase_id}/${stakeholder}`);
      return view(stakeholder, phase_id);
    }
    async merge({ phase_id }) {
      await new Promise((r) => setTimeout(r, 5));
      this.calls.push(`${phase_id}/merge`);
      return merge(phase_id);
    }
  }
  const a = new Slow();
  const res = await new sv.Council('An idea', a, { roster: ROSTER, phases: PHASES }).run();
  assert.deepStrictEqual(a.calls, ['frame/Dev', 'frame/PO', 'frame/merge', 'build/Dev', 'build/PO', 'build/merge']);
  assert.deepStrictEqual(sv.health(res).merges, { full: 2 });
  assert.deepStrictEqual(res.rounds[0].views[0].missing, ['one thing']);
  assert.deepStrictEqual(res.locked_decisions, ['frame: resolved', 'build: resolved']);
  assert.strictEqual(res.synthetic, false);
}

async function thrownValues() {
  const thrown = [
    'plain string', 42, null, undefined, { code: 7 }, Symbol('s'), Object.create(null),
    new TypeError('bad type'), Promise.reject(new RangeError('async range')),
  ];
  class Custom extends Error {}
  thrown.push(new Custom('custom class'));
  const err = (t) => async () => {
    if (t instanceof Promise) return t;
    throw t;
  };
  const got = [];
  for (const t of thrown) {
    if (t instanceof Promise) t.catch(() => {});
    const a = { name: 'thrower', view: err(t), merge: () => merge('frame') };
    const res = await new sv.Council('An idea', a, { roster: ['Dev'], phases: [PHASES[0]], retries: 0 }).run();
    assert.strictEqual(res.rounds[0].views[0].status, 'failed');
    assert.strictEqual(res.rounds[0].juror.synthesis_status, 'no_views');
    got.push(res.rounds[0].views[0].error);
  }
  assert.deepStrictEqual(got, [
    'Error: plain string', 'Error: 42', 'Error: null', 'Error: undefined', 'Error: [object Object]', 'Error: Symbol(s)',
    'Error: (message not printable)', 'TypeError: bad type', 'RangeError: async range', 'Custom: custom class',
  ]);
  // a synchronous throw from a non-async method, and the 500 code point cut
  const long = '🙂'.repeat(600);
  const a = { name: 'sync', view: () => { throw new sv.AdapterError(long); }, merge: () => merge('frame') };
  const res = await new sv.Council('An idea', a, { roster: ['Dev'], phases: [PHASES[0]] }).run();
  const e = res.rounds[0].views[0].error;
  assert.strictEqual([...e].length, 500);
  assert.ok(e.startsWith('AdapterError: 🙂'));
  // secrets are redacted, lone surrogates become '?', an unprintable message is named so
  const errs = [];
  const bad = { toString() { throw new Error('no'); } };
  const getterMsg = new Error('x');
  Object.defineProperty(getterMsg, 'message', { get() { throw new Error('no'); } });
  for (const t of [new Error('call failed: api_key=sk-123 and Bearer abcdefghijkl'), new Error('a\ud800b'), bad, getterMsg]) {
    const r = await new sv.Council('An idea', { name: 'x', view: () => { throw t; }, merge: () => merge('frame') },
      { roster: ['Dev'], phases: [PHASES[0]], retries: 0 }).run();
    errs.push(r.rounds[0].views[0].error);
  }
  assert.deepStrictEqual(errs, ['Error: call failed: api_key=*** and Bearer ***', 'Error: a?b',
    'Error: (message not printable)', 'Error: (message not printable)']);
}

async function notPlain() {
  // answers that are not plain JSON data are refused before validation, with Python's messages where they exist
  class MyView extends Object {}
  const withGetter = view('Dev', 'frame');
  Object.defineProperty(withGetter, 'contribution', { enumerable: true, get: () => 'changes every time' });
  const deep = view('Dev', 'frame');
  let d = 'leaf';
  for (let i = 0; i < 40; i++) d = [d];
  deep.missing = d;
  const sym = view('Dev', 'frame');
  sym[Symbol('s')] = 1;
  const big = view('Dev', 'frame');
  big.missing = Array.from({ length: 3 }, () => 'x'.repeat(400000));
  const cases = [
    [new Map(), 'invalid: Map is not JSON data'],
    [Object.assign(new MyView(), view('Dev', 'frame')), 'invalid: MyView is not JSON data'],
    [withGetter, 'invalid: getters are not JSON data'],
    [deep, 'invalid: nested deeper than 32 levels'],
    [sym, 'invalid: object keys must be strings'],
    [big, 'invalid: more than 1,000,000 characters of text'],
    [{ ...view('Dev', 'frame'), done_so_far: undefined }, 'invalid: undefined is not JSON data'],
    [{ ...view('Dev', 'frame'), done_so_far: 1n }, 'invalid: bigint is not JSON data'],
  ];
  for (const [ans, want] of cases) {
    const r = await new sv.Council('An idea', { name: 'x', view: () => ans, merge: () => merge('frame') },
      { roster: ['Dev'], phases: [PHASES[0]], retries: 0 }).run();
    assert.strictEqual(r.rounds[0].views[0].error, want);
  }
  // a Proxy that answers differently on each read: the engine uses the one copy it validated
  let reads = 0;
  const lying = new Proxy(view('Dev', 'frame'), {
    getOwnPropertyDescriptor(target, k) {
      const desc = Reflect.getOwnPropertyDescriptor(target, k);
      if (k === 'devils_advocate') desc.value = reads++ ? '' : 'a real risk';
      return desc;
    },
  });
  const r = await new sv.Council('An idea', { name: 'x', view: () => lying, merge: () => merge('frame') },
    { roster: ['Dev'], phases: [PHASES[0]], retries: 0 }).run();
  assert.strictEqual(r.rounds[0].views[0].status, 'ok');
  assert.strictEqual(r.rounds[0].views[0].devils_advocate, 'a real risk');
  // an unknown "__proto__" key is data: reported as unknown, never sets a prototype
  const proto = JSON.parse('{"__proto__": {"polluted": true}}');
  Object.assign(proto, view('Dev', 'frame'));
  assert.deepStrictEqual(sv.checkView(sv.plain(proto)), ['unknown keys (1): __proto__']);
  assert.strictEqual(({}).polluted, undefined);
}

async function inputChecks() {
  // unpaired surrogates in the idea, roster, sources or phases are refused before the run
  const msg = 'the idea, roster, phases and sources must not contain unpaired surrogates';
  const stub = new sv.StubAdapter();
  for (const [idea, o] of [['a\ud800', {}], ['ok', { roster: ['Dev', '\udc00'] }], ['ok', { sources: ['s\ud83d'] }],
    ['ok', { phases: [['frame', 'Frame', 'Q\ud800?']] }], ['ok', { phases: [{ step_id: 'f', title: 'F\udfff', question: 'Q' }] }]]) {
    assert.throws(() => new sv.Council(idea, stub, o), { message: msg });
  }
  assert.doesNotThrow(() => new sv.Council('ok 🙂', stub, { roster: ['Dev 🙂'] }));
  // readJsonl stops after 1,000,000 lines, empty lines included
  const lines = Array(1000005).fill('');
  lines[1000000] = '{"output": "x"}';
  assert.deepStrictEqual(sv.readJsonl(lines), []);
  assert.deepStrictEqual(sv.readJsonl(['', '{"output": "x"}', 'not json']), [{ output: 'x' }, null]);
}

async function protoData() {
  // replayed data with "__proto__" keys
  const rec = JSON.parse(JSON.stringify(await new sv.Council('An idea', new sv.StubAdapter(),
    { roster: ROSTER, phases: [PHASES[0]] }).run()));
  rec.rounds[0].views[0].w5h1 = JSON.parse('{"who":"a","what":"b","where":"c","when":"d","why":"e","how":"f","__proto__":{"x":1}}');
  const again = await new sv.Council('An idea', new sv.ReplayAdapter(rec), { roster: ROSTER, phases: [PHASES[0]] }).run();
  assert.strictEqual(again.rounds[0].views[0].status, 'failed');
  assert.ok(again.rounds[0].views[0].error.startsWith('invalid: w5h1 must be an object with exactly'));
  // imported data with "__proto__" keys, string stakeholders and a non-list stakeholders value
  const doc = JSON.parse(`{"idea": {"__proto__": "x", "name": "N"}, "stakeholders": "Dev, PO",
    "rounds": [{"step_id": "frame", "views": [{"stakeholder": "Dev", "contribution": {"__proto__": "p", "a": 1}}],
    "juror": {"__proto__": {"synthesis_status": "full"}}}]}`);
  const imp = sv.fromSmartsignal(doc);
  assert.strictEqual(imp.idea, 'N');
  assert.deepStrictEqual(imp.stakeholders, ['Dev']);
  assert.strictEqual(imp.rounds[0].views[0].contribution, '__proto__: p\na: 1');
  assert.strictEqual(imp.rounds[0].juror.synthesis_status, 'failed');
  const imp2 = sv.fromSmartsignal({ ...doc, stakeholders: ['A', { name: 'B' }, 3] });
  assert.deepStrictEqual(imp2.stakeholders, ['A', 'B', '3']);
  // trainset kinds that are Object.prototype names
  const rep = sv.auditTrainset([{ output: 'a', meta: { kind: 'constructor' } }, { output: 'b', meta: { kind: 'constructor' } },
    JSON.parse('{"output": "c", "meta": {"kind": "__proto__"}}'), { output: 'd', meta: { kind: 'toString' } }, 'not a row']);
  assert.deepStrictEqual(rep.summary.kinds, [['__proto__', 1], ['constructor', 2], ['toString', 1]]);
  assert.strictEqual(rep.findings[0].code, 'AUDIT-TRAINSET-UNREADABLE');
  assert.strictEqual(({}).polluted, undefined);
}

async function plainViews() {
  let seen = null;
  class Inspect extends sv.StubAdapter {
    merge(args) {
      seen = args;
      args.views[0].contribution = 'mutated by the adapter';
      args.context.locked.push('mutated');
      return super.merge(args);
    }
  }
  const res = await new sv.Council('An idea', new Inspect(), { roster: ROSTER, phases: [PHASES[0]] }).run();
  const isPlain = (o) => Object.getPrototypeOf(o) === Object.prototype;
  assert.strictEqual(seen.views.length, 2);
  for (const v of seen.views) {
    assert.ok(isPlain(v) && isPlain(v.w5h1) && Array.isArray(v.missing));
    assert.deepStrictEqual(Object.keys(v), ['stakeholder', 'role', 'status', 'w5h1', 'done_so_far', 'missing',
      'devils_advocate', 'contribution', 'sources', 'error']);
    assert.strictEqual(v.status, 'ok');
  }
  assert.ok(isPlain(seen.context));
  assert.deepStrictEqual(Object.keys(seen.context), ['idea', 'iteration', 'sources', 'prior', 'locked']);
  // the adapter got copies: the run's records are unchanged
  assert.notStrictEqual(res.rounds[0].views[0].contribution, 'mutated by the adapter');
  assert.ok(!res.locked_decisions.includes('mutated'));
}

async function replay() {
  const script = { views: { 'frame|1|PO': [{ raise: 'down' }] } };
  const res = await new sv.Council('An idea', new sv.ScriptedAdapter(script), { roster: ROSTER, phases: PHASES }).run();
  const again = await new sv.Council('An idea', new sv.ReplayAdapter(JSON.parse(JSON.stringify(res))),
    { roster: ROSTER, phases: PHASES }).run();
  assert.deepStrictEqual(sv.health(again), sv.health(res));
  assert.strictEqual(again.rounds[0].views[1].error, 'ReplayError: no recorded valid view for PO at frame');
}

(async () => {
  for (const [name, t] of [['async adapter', asyncAdapter], ['thrown values', thrownValues],
    ['plain views', plainViews], ['replay', replay], ['not plain', notPlain], ['__proto__ as data', protoData], ['input checks', inputChecks]]) {
    try {
      await t();
    } catch (e) {
      console.log(`${name}: ${e.stack || e}`);
      console.log('node: adapter tests FAILED');
      process.exit(1);
    }
  }
  console.log('node: adapter tests passed');
})();
