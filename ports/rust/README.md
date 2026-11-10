# SmartSavant for Rust

This is the SmartSavant council engine in Rust. It contains the parts of `spec/SEMANTICS.md` that the
shared vectors test: canonical JSON and SHA-256, the gates, the schema checks (with `plain` and the exact
error strings), the engine, the stub, scripted and replay adapters, the exporters (stories, spec Markdown,
Mermaid, trainset), the SmartSignal importer and the audit. A Rust program runs it in-process and
supplies its own adapter, which calls its own model. The command line, the demos, zip trainsets and the
file writers stay in the Python reference.

The crate (`sf-smartsavant`, library name `smartsavant`) has no dependencies (empty `[dependencies]`) and
builds with `--offline`. It is not published on crates.io (`publish = false`); use it from a clone.

## Layout

| Path | What |
|---|---|
| `src/json.rs` | `Value`, `Object` (keeps key order), `parse`, `canonical`, `py_dumps` (Python's `json.dumps`), `equal` |
| `src/sha256.rs` | SHA-256 (own implementation; the standard library has none) |
| `src/py.rs` | the Python behaviour the reference relies on: code-point `len` and `head`, `to_str`, `repr`, `truthy`, `lower`, `strip` |
| `src/gates.rs` | `squash`, `blank`, `redact`, `figures`, `flag_unsourced`, `is_generic`, `is_fallback`, `has_placeholder_code`, `synthesis_ok`, `compliance_block` |
| `src/schema.rs` | `plain`, `check_view`, `check_merge`, `normalize_view`, `check_council` |
| `src/model.rs` | `CouncilResult` and its parts, `to_value` (council.json form), `CouncilResult::from_value` |
| `src/adapters.rs` | `Adapter`, `AdapterError`, `Context`, `StubAdapter`, `ScriptedAdapter`, `ReplayAdapter` |
| `src/engine.rs` | `Council::new`, `Council::run`, `Options`, `Phase`, `default_phases`, `check_phases`, `load_phases`, `health`, `error_text` |
| `src/exporters.rs` | `run_digest`, `to_stories`, `to_spec_md`, `to_trainset`, `to_mermaid`, `md`, `mermaid_label` |
| `src/importer.rs` | `from_smartsignal`, `detect`, `idea_text` |
| `src/audit.rs` | `audit_result`, `audit_stories`, `audit_trainset`, `read_jsonl`, `audit_path` (`.json` and `.jsonl`) |
| `src/conformance.rs` | `run_dir`, shared by the runner and the tests |
| `src/bin/conformance.rs` | the vector runner |
| `tests/engine.rs` | the vectors, plus error text, panics, `plain` limits, unpaired surrogates, replay |

## How to run

    cd ports/rust
    cargo run --quiet --offline --release --bin conformance ../../spec/conformance
    # rust: 198/198 vectors passed
    cargo test --offline

From the repository root, `ports/conformance/run.sh rust` runs the vectors through the shared harness.

## Using it

Add the crate from a clone as a path dependency:

    [dependencies]
    sf-smartsavant = { path = "../sf-smartsavant/ports/rust" }

```rust
use smartsavant::{exporters, audit, health, Council, Options, Phase, StubAdapter};

let opts = Options {
    roster: vec!["Developer".into(), "UX/UI".into(), "Product Owner".into()], // empty: the default roster
    phases: vec![Phase::new("frame", "Frame", "What is the problem?")],       // empty: the default phases
    sources: vec!["survey-2026".into()],                                      // source ids views may cite
    ..Default::default()                                                      // max_loops 1, retries 1
};
let mut council = Council::new("Learn reusable automations from recorded operator sessions", StubAdapter, opts)?;
let res = council.run();                       // CouncilResult; res.to_value() is the council.json object

health(&res);                                  // counts; decided only when every phase has a full synthesis
exporters::to_spec_md(&res);                   // Markdown; undecided phases are marked NOT DECIDED
exporters::to_stories(&res, "PROJ");           // ready or blocked stories
exporters::to_trainset(&res, false);           // methodology rows only for a synthetic run such as this one
audit::audit_result(&res).verdict;             // "not build-ready": the stub is synthetic
smartsavant::json::py_dumps(&res.to_value(), false);
```

## Writing an adapter

```rust
use smartsavant::{Adapter, AdapterError, MergeRequest, Value, ViewRequest};

struct MyModel { /* client, endpoint ... */ }

impl Adapter for MyModel {
    fn name(&self) -> String { "my-model".into() }      // written into council.json
    fn synthetic(&self) -> bool { false }                // true for stubs and demos
    fn view(&mut self, req: &ViewRequest) -> Result<Value, AdapterError> {
        // req.stakeholder, req.role, req.phase_id, req.question, req.context (idea, iteration, sources,
        // prior, locked)
        let text = call_my_model(req).map_err(|e| AdapterError::new("HTTPError", e.to_string()))?;
        smartsavant::json::parse(&text).map_err(|e| AdapterError::new("ValueError", e))
    }
    fn merge(&mut self, req: &MergeRequest) -> Result<Value, AdapterError> {
        // req.views are the valid views of the phase, in roster order
        todo!()
    }
}
```

The engine copies each answer with `plain`, validates it with `check_view` / `check_merge`, and retries
an invalid answer or an `Err` `retries` times; then it records the view as `failed` (or the merge as
`failed`) with the error text, and never invents content. Each call gets a fresh request. The recorded
error is `"<name>: <message>"` from the `AdapterError`, secrets redacted, at most 500 code points. A panic
in `view` or `merge` is caught and recorded as `Panic: <message>` (the default panic hook still prints it
to stderr).

Treat the idea and the sources as untrusted data in your prompts; `spec/prompts/` has the contracts.

## What was tested

rustc and cargo 1.97.0 on Linux x86-64: the 198 vectors (well under a second in a release build),
`cargo test`, `cargo clippy` without warnings, and a differential comparison with the Python reference
(Python 3.13.16): about 44,000 random strings through `figures`, `is_generic`, `is_fallback`,
`has_placeholder_code`, `squash`, `redact`, `compliance_block`, `md` and `mermaid_label`, and eight sets
of about 1,200 random vectors each (scripted runs with invalid answers, errors, loops and sources;
SmartSignal exports with values of every JSON type; stories; trainset rows; view and merge checks). All
matched. That generator is not part of the repository.

## Limits and differences

- **Unpaired surrogates.** Rust strings hold UTF-8, which cannot encode U+D800-U+DFFF. `parse` maps an
  unpaired surrogate escape (`"\ud800"`) to the code point U+10F800 + (s - 0xD800), in the last 2048 code
  points of plane 16 (private use). The checks find it there (`contains an unpaired surrogate`), the
  importer and the unknown-key error text replace it with U+FFFD, adapter error text with `?`, and the writers (`canonical`, `py_dumps`,
  `repr`) write it back as a `\udXXX` escape. Differences: a real character from U+10F800-U+10FFFF in
  the input is treated as an unpaired surrogate; Python writes a lone surrogate raw in `json.dumps` and
  cannot encode it in `canonical` at all; keys holding one sort after other characters here and before
  them in Python.
- `repr` (used by `str()` of non-string values, e.g. the counts in the spec header) has no Unicode
  category tables: it escapes controls, separators other than U+0020, format characters and private-use
  characters from a built-in list, and takes unassigned code points as printable (Python escapes them).
- Integers outside the i64 range are kept as `Value::BigInt` (their decimal text).
- `parse` refuses nesting deeper than 1,000 levels.
- An adapter returns a `Value`, which is JSON data by construction, so `plain` only enforces the depth
  and text limits; the reference's "is not JSON data" refusal has no counterpart.
- Not included: the command line, the demos, the file writers (`write_all`), `load_adapter`, and zip
  trainsets (`audit_path` reads `.json` and `.jsonl`).
