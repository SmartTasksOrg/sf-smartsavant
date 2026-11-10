# SmartSavant for Node.js

This is the SmartSavant council engine in JavaScript for Node.js. It contains the parts of
`spec/SEMANTICS.md` that the shared vectors test: the engine, the gates, the schema checks, the stub,
scripted and replay adapters, the exporters (stories, spec Markdown, Mermaid, trainset), the SmartSignal
importer and the audit. An Electron or Node app runs it in-process and supplies its own adapter, which
calls its own model endpoint. The command line, the demos, zip trainsets and the file writers stay in the
Python reference.

It uses only the Node.js standard library (`crypto`, `fs`, `path`) and has no dependencies. It is not
published on npm or any other registry (`"private": true`). Use it from a clone of this repository.

## Layout

| Path | What |
|---|---|
| `src/canonical.js` | canonical JSON (`dumps`, `encode`), `sha256Hex` |
| `src/gates.js` | `squash`, `blank`, `redact`, `figures`, `flagUnsourced`, `isGeneric`, `isFallback`, `hasPlaceholderCode`, `synthesisOk`, `complianceBlock` |
| `src/schema.js` | `plain` (`NotPlainData`), `checkView`, `checkMerge`, `normalizeView`, `checkCouncil` |
| `src/model.js` | plain-object constructors for views, merges, rounds and results; `resultFromDict` |
| `src/engine.js` | `Council` (async `run()`), `health`, `loadPhases`, `checkPhases`, `DEFAULT_ROSTER`, `DEFAULT_PHASES` |
| `src/adapters/` | `LLMAdapter` (base), `StubAdapter`, `ScriptedAdapter` (`AdapterError`), `ReplayAdapter` (`ReplayError`) |
| `src/exporters.js` | `runDigest`, `toStories`, `toSpecMd`, `toTrainset`, `toMermaid` |
| `src/importer.js` | `fromSmartsignal`, `detect`, `ideaText` |
| `src/audit.js` | `auditResult`, `auditStories`, `auditTrainset`, `auditPath` (`.json` and `.jsonl`), `readJsonl`, `render`, `esc` |
| `src/py.js` | the Python behaviour the reference relies on: code-point lengths, `str()`, `json.dumps()`, lone surrogates |
| `src/index.js` | exports all of the above |
| `test/conformance.js` | the vector runner |
| `test/adapters.js` | async adapters, thrown non-Error values, error text, answers that are not plain data, the shape of what `merge()` receives, replay, `__proto__` and `constructor` as data, unpaired surrogates in the inputs, the `readJsonl` line budget |

## How to run

From the repository root:

```sh
node ports/node/test/conformance.js spec/conformance   # last line: node: 198/198 vectors passed
node ports/node/test/adapters.js                       # node: adapter tests passed
ports/conformance/run.sh node                          # the vectors through the shared harness
```

`npm test` in `ports/node` runs both test files.

## Using it

```js
const sv = require('./ports/node/src');

const council = new sv.Council('Learn reusable automations from recorded operator sessions', new sv.StubAdapter(), {
  roster: ['Developer', 'UX/UI', 'Product Owner'],   // default: the four of DEFAULT_ROSTER
  phases: [['frame', 'Frame', 'What is the problem?'], ['build', 'Build', 'What is the technical approach?']],
  sources: ['survey-2026'],                          // source ids views may cite
  maxLoops: 2,                                       // 1 to 10, default 1
  retries: 1,                                        // 0 to 5, default 1
});
const res = await council.run();                     // a plain CouncilResult object (bindings/council.d.ts)

sv.health(res);                // counts; decided is true only when every phase has a full synthesis
sv.toSpecMd(res);              // Markdown; undecided phases are marked NOT DECIDED
sv.toStories(res, 'PROJ');     // ready or blocked stories
sv.toTrainset(res);            // methodology rows only for a synthetic run such as this one
sv.auditResult(res);           // verdict "not build-ready": a stub run is synthetic
```

`phases` also accepts `{step_id, title, question}` objects. Phases are checked (`sv.checkPhases`): three
non-blank strings each and unique step ids. The idea, roster names, sources and phase strings must not contain unpaired
UTF-16 surrogates; the constructor throws otherwise. `sv.loadPhases(file)` reads a phases file in the format of
`spec/phases.json`. The result and everything the exporters return are plain JSON
values: `JSON.stringify(res)` is a council.json that the Python reference reads.

## Writing an adapter

An adapter has a `name`, a `synthetic` flag and two methods, `view` and `merge`. Each method gets one
object argument and returns the answer or a promise of it. The engine:

- calls them one at a time, in roster order, so an adapter does not need to handle concurrent calls;
- gives each call a fresh deep copy of its arguments (`context` is `{idea, iteration, sources, prior,
  locked}`; `views` are the phase's valid views as plain objects);
- first copies every answer to plain data (`sv.plain`), then validates the copy with `checkView` or
  `checkMerge` and uses only the copy;
- retries a call that throws or returns an invalid answer;
- records a call that never succeeds as `failed` with its error, for example
  `"TypeError: fetch failed"` or `"invalid: missing keys: sources"`. Each validation problem is cut to 200
  code points and the whole error to 500.

`plain` accepts `null`, booleans, numbers, strings, arrays and plain objects (prototype `Object.prototype`
or `null`). It reads every property once, through its property descriptor, so a getter or a `Proxy`
cannot show the check one value and the engine another. It refuses, as `"invalid: <message>"`:

| Answer | Message |
|---|---|
| nesting deeper than 32 levels | `nested deeper than 32 levels` |
| more than 1,000,000 code points of text, keys included | `more than 1,000,000 characters of text` |
| a symbol key | `object keys must be strings` |
| another type or a subclass (`Map`, `Date`, a class instance, a subclass of `Array`, `undefined`, a function, a BigInt) | `<type name> is not JSON data`, for example `Map is not JSON data` or `undefined is not JSON data` |
| a getter or setter, or an array hole | `getters are not JSON data` or `undefined is not JSON data` |

The first three are Python's messages. The others are specific to JavaScript. A key named `__proto__` is
kept as an ordinary key (and reported as an unknown key); it never changes a prototype.

The answer formats are in `bindings/council.d.ts`, `spec/schema/view.schema.json` and
`spec/schema/merge.schema.json`. The prompt contracts are in `spec/prompts/`. A string is blank when it is empty or made only of
`[ \t\n\r\f\v]` (`sv.blank`); other Unicode spaces such as U+00A0 and U+3000 count as content. The idea, the sources and
the earlier decisions are untrusted text, so keep them out of your instructions and pass them as data.

The example below posts to a model endpoint of your own. **The URL is an example placeholder**; replace it
with your endpoint, and adapt the request and response handling to your API.

```js
const sv = require('./ports/node/src');

// EXAMPLE ONLY: replace with your own model endpoint.
const ENDPOINT = 'https://llm.example.invalid/v1/council';

class MyEndpointAdapter extends sv.LLMAdapter {
  constructor(token) {
    super();
    this.name = 'my-endpoint';
    this.synthetic = false;    // true only for stubs and demos
    this.token = token;
  }

  async call(task, payload) {
    const r = await fetch(ENDPOINT, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${this.token}` },
      body: JSON.stringify({ task, ...payload }),
      signal: AbortSignal.timeout(60000),
    });
    if (!r.ok) throw new Error(`HTTP ${r.status}`);   // recorded as "Error: HTTP 503", then retried
    return r.json();                                  // must be the answer object itself
  }

  view({ stakeholder, role, phase_id, question, context }) {
    return this.call('view', { stakeholder, role, phase_id, question, context });
  }

  merge({ phase_id, question, views, context }) {
    return this.call('merge', { phase_id, question, views, context });
  }
}

const res = await new sv.Council('An idea', new MyEndpointAdapter(process.env.MY_TOKEN)).run();
```

A subclass of `LLMAdapter` is not required: any object with `view` and `merge` methods works.

Errors are recorded as `"<name>: <message>"`. For an `Error` the name is its `name` property, or its class
name when `name` is the generic `"Error"`, so a class `TimeoutError extends Error` gives
`"TimeoutError: ..."`. A thrown value that is not an `Error` (a string, a number, `null`, a plain object)
gives `"Error: "` followed by `String(value)`; the engine handles it like any other failure. A message
that cannot be made a string (its getter throws, or `String()` throws, as for `Object.create(null)`) gives
`"(message not printable)"`. Unpaired surrogates in the message become `?`, as in Python. Then
`key=VALUE`-style secrets and bearer tokens are replaced by `***` (`sv.redact`), and the text is cut to 500
code points. For example `new Error('HTTP 401 for api_key=sk-123')` is recorded as
`"Error: HTTP 401 for api_key=***"`.

## What was tested

Node 22.22.0 on Linux x86-64, with the Python reference on Python 3.13.16:

- the 198 vectors;
- `test/adapters.js`;
- a differential run against the Python reference: random strings and objects through the gates,
  `checkView`, `checkMerge` and `flagUnsourced`; random SmartSignal exports (wrong types, `__proto__` and
  `constructor` keys, deep nesting) through the importer and the audit; random trainset rows through
  `auditTrainset`; random exception messages through the error text; and 300 random councils (scripted
  failures, retries, loops, flags, non-ASCII text) through every exporter and the audit. The only
  differences left are the JSON parsing limits listed below.

`package.json` declares Node >= 18 (the engine needs regex lookbehind, named groups and the global
`fetch` for the example above). Versions other than 22 were not tested.

## Limits

- JSON is parsed with `JSON.parse`, so a float with an integral value (`1.0`, `123.0`, `1.5e16`) becomes
  an integer. Valid adapter answers contain no numbers, so runs are not affected, but imports are:
  - an imported `"iteration": 1.0` (or `"round": 1.0`) counts as the integer 1 here, so the round becomes
    iteration 2; Python accepts only a real integer and gives iteration 1;
  - where the importer turns a number into text, Python writes `123.0` and this port writes `123`;
  - a council file with `"iteration": 1.0` passes `checkCouncil` here and fails it in Python.
- JavaScript objects list integer-like keys (`"5"`) first, whatever their order in the JSON text. Where
  the reference uses the input order, the order can differ from Python's: the importer flattening an
  object to `key: value` lines, and the flags of a view whose `sources` is an object instead of a list.
- `str()` of a dict or list in the rare fallbacks of the audit follows Python's `repr()`; whether a
  non-ASCII character is printable comes from Node's Unicode tables, which can differ from Python's for
  recently assigned characters.
- The stub's tag hashes the UTF-8 of the idea. `Council` refuses an idea with an unpaired surrogate, as
  Python does, but a direct call of `StubAdapter.view` with one encodes it as U+FFFD; Python raises.
- `auditPath` reads `.json` and `.jsonl` files only (the `.jsonl` row, size and line limits are as in
  Python); zip trainsets are read by the Python reference. Node's `JSON.parse` reads very deep nesting
  that Python's `json` refuses ("JSON nested too deeply"), so such a file is audited here (the importer
  stops at depth 20, as in Python) where Python raises an error.
- The command line, the demos, `load_adapter`, `write_all` and `write_new` are not ported.
