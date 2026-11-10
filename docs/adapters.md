# Writing an adapter

An adapter is the only code that talks to a model. Subclass `sf_smartsavant.LLMAdapter`, set `name`,
and implement two methods. The engine calls them one at a time, in roster order.

```python
from sf_smartsavant import LLMAdapter

class ModelAdapter(LLMAdapter):
    name = "my-model"          # recorded in council.json
    synthetic = False          # True only for stubs and demos: their runs are never trained or built on

    def view(self, *, stakeholder, role, phase_id, question, context): ...
    def merge(self, *, phase_id, question, views, context): ...
```

## The two calls

`view` returns one stakeholder's view of one phase. These are the exact rules in spec/SEMANTICS.md
section 3, and the schema is in `spec/schema/view.schema.json`:

```json
{"w5h1": {"who": "", "what": "", "where": "", "when": "", "why": "", "how": ""},
 "done_so_far": "", "missing": ["..."], "devils_advocate": "a concrete risk (required)",
 "contribution": "", "sources": ["ids the run allows"]}
```

`merge` returns the juror's decision for the phase (`spec/schema/merge.schema.json`):

```json
{"merge_summary": "required", "decision": "", "resolved": ["topic: decision"], "open_issues": [],
 "next_steps": ["<Verb> <the real component>"], "reversals": [], "spec_delta": "", "loop_recommended": false}
```

`context` holds:
- `idea`;
- `iteration`;
- `sources` (the ids views may cite);
- `prior` (earlier decisions);
- `locked` (the "topic: decision" entries settled so far).

`views` are the valid views of this phase. Both are copies, so changing them changes nothing in the
run.

## What the engine does with an answer

- **It checks the answer.** The answer is first copied to plain JSON data. Subclasses of dict, str or
  list, other types and very large or deep answers are refused. Then extra keys, wrong types, empty
  required text, too-long strings (8000 code points), lists longer than 20 items, invalid source ids
  and unpaired surrogates all make it invalid.
- **It retries** an invalid answer or an exception `retries` times (default 1). If the answer never
  becomes valid, it records `failed` with the error. It never invents a view or a decision.
- **It judges each valid merge.** A merge is `full` only when it decides something: a decision and a
  summary that are not fallback text, a resolved entry, and a next step that names something real.
  Otherwise the merge is `thin`, and the phase counts as not decided.
- **It flags figures.** A figure ("30%", "5 ms", "$2k") in a view that cites no source is flagged, and
  so is a cited source the run did not provide (with no `--sources`, every cited source is flagged). The view is kept and flagged; the trainset skips it.

## Recommendations

- **Constrain decoding** to the JSON schemas (JSON-schema or grammar-constrained output), if your
  serving stack supports it. Never regex-salvage a broken answer into a "merge": return what the model
  said, or raise, and let the engine record the failure. Salvaged text that looks like a decision is
  how a council produces stories that say nothing.
- **Mark untrusted data.** The idea, the sources and earlier views are data, not instructions. Put them
  between clear markers in your prompt and tell the model not to follow instructions inside them. The
  engine's validation limits what a manipulated answer can contain; it cannot make a manipulated
  answer true.
- **Set timeouts** in your adapter (the engine waits as long as your call takes). Raise on a timeout;
  the engine retries and then records the failure.
- **Keep secrets in the adapter's environment**, never in `--adapter-arg` (process lists show it) and
  never in `council.json`.
- **Use the scripted adapter to test your prompts' failure handling.** `ScriptedAdapter` lets you
  script exact answers and exceptions per call. `ReplayAdapter` replays a recorded run, for example to
  re-export it after a gate changes.

The schemas ship in the package: `sf_smartsavant._bundled.schema_path('view')` and `('merge')`.

A skeleton for an OpenAI-compatible endpoint is in `examples/adapter_skeleton.py`. It uses the
standard library only; the URL and the model name are placeholders for your own.
