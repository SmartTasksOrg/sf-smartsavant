# Decisions

## Where SmartSavant 0.1.0 comes from

The SmartSavant council run (October 2026) did not produce a specification. All of its stakeholder
views came from offline templates, and all of its juror merges failed and were replaced with fallback
text (docs/audit.md). The maintainers then supplied `smartsavant-core` 0.1.0: a small reference
implementation of the council engine itself, with a model-agnostic adapter, gates and exporters. This
package is that core, hardened, tested and specified. The design decisions are the maintainers' and
this repository's, not the council's.

## Changes from smartsavant-core 0.1.0

| Core 0.1.0 | sf-smartsavant 0.1.0 | Why |
|---|---|---|
| Adapter answers used as returned; the README asked adapters to validate | The engine validates every answer (spec/schema/, exact rules in SEMANTICS), retries, and records `failed` | The council run showed what happens when broken answers are salvaged into "decisions". |
| An adapter exception stopped the run | Exceptions are recorded per view and merge; the run continues | One failing call should not lose the other phases. |
| No status per view or merge | `status` (ok / failed / offline) and `synthesis_status` (full / thin / recovered / failed / offline / no_views) | Exports and audits need to know what was decided. |
| Views passed to merge as `__dict__` (with a dataclass inside: not JSON-serialisable for a real adapter) | Plain dicts, deep-copied for every call | A real adapter serialises the views; adapters must not change the run. |
| `max_loops` default 1 with the loop check after recording | The same default; loops repeat a phase up to `max_loops` and see earlier decisions | Unchanged behaviour, now specified and tested. |
| `flag_unsourced`: any run-level source silenced every flag; any digit was a "figure" | Per view: no cited source flags each figure; unknown cited ids are flagged; figures need a unit, currency, % or separator | Run-level sources do not make a view's figure sourced, and "2-5 steps" is not a figure. |
| `synthesis_ok`: decision + summary + one resolved entry | Also: status full, no fallback text, at least one non-generic next step | The council run's merges had text in every field and decided nothing. |
| Stories titled "Advance the <phase> with the decided approach" | Story title = the first real next step; undecided phases are `blocked` stories | Those titles were the 9 stories of the failed run. |
| Trainset: methodology output = the phase question; facilitation from any merge passing the gate | Fixed phase guidance; facilitation and expert rows only from non-synthetic runs, full merges and unflagged views; provenance in every row | A trainset learned from stubs or fallbacks teaches the model to fail. |
| Spec Markdown with raw model text | Model text is made inert (whitespace collapsed, `<` and `>` escaped, `|` in tables); undecided phases marked | Model text must not inject markup into a document people read. |
| No import, no audit, no command line | SmartSignal importer, `audit`, `run`, `export`, `validate`, `demo`, `vectors` | To check runs that already exist, from the shell or CI. |
| `bindings/council.d.ts` | Updated to the 0.1.0 contract; ports in Node.js, Go, Rust, Java, C#, PHP and Ruby pass the same vectors | The core named Node/Electron as the first port target; the other six are the languages of the SmartWatch ports. |
| `compliance_block`: one hard-coded rule | Rules as data, two default rules, documented as a keyword stub | A private build plugs in its own policy. |

The core's public/private split stays:
- **Public:** the engine, the schemas, the gates, the exporters, and the stub, scripted and replay
  adapters.
- **Private:** your real adapter, your tuned prompts and your policy gate.

## Not done

- **Real adapters, by design.** `examples/adapter_skeleton.py` shows the shape.
- **A service, a user interface, persistence.**
- **The SmartTasks plugin integration** (recon321): that belongs in the SmartTasks app repository.
- **Publishing the ports.** They are used from a clone; no registry name is held (see `names.json`).
