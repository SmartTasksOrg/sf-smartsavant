# Changelog

All notable changes to SmartSavant are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versioning is semantic.

## Unreleased

## 0.1.0 — first release as `sf-smartsavant`

Built from smartsavant-core 0.1.0 (the maintainers' reference core); docs/decisions.md lists every
change.

### Added
- Council engine:
  - roster, then phases (spec/phases.json), then views, then a juror merge, then the decision tree;
  - loops up to `max_loops`;
  - locked decisions and earlier decisions passed to later phases;
  - every adapter answer validated, retried and recorded as `failed` when it never becomes valid;
  - synthesis statuses full, thin, failed and no_views.
- Adapters: `LLMAdapter`, a deterministic `StubAdapter`, a `ScriptedAdapter` for tests and a
  `ReplayAdapter` for recorded runs.
- Gates: figures, unsourced figures and unknown sources, generic steps, fallback text, placeholder
  code, synthesis check, keyword compliance rules.
- Exporters: stories (blocked when undecided), spec Markdown (model text made inert), trainset (never
  from synthetic runs, failed merges or flagged views), Mermaid decision tree, run digest.
- Importer for SmartSignal council exports (results and rounds).
- Audit of council runs, stories and trainsets, with stable finding codes and a build-ready verdict.
- Command line: run, audit, import, export, validate, demo, vectors.
- JSON schemas for adapter answers, the spec and 198 shared vectors.
- Ports that pass the 198 vectors, each with the standard library only: Node.js, Go, Rust, Java, C#,
  PHP and Ruby (`ports/`). None is published to a registry.
- CI: `ports` (Python and Node, gating) and `ports-all` (the other six ports, not gating yet).

### Fixed before release (pre-release review and the private robustness suite; tests in tests/test_review_fixes.py)
- Fallback text in resolved entries or next steps no longer counts as a full synthesis. The audit scans
  those fields too, and generic-step detection covers more variants.
- The audit recomputes the synthesis gate instead of trusting a recorded `full`, and treats the
  `stub`, `scripted` and `demo*` adapters as synthetic. Garbage trainset rows are an error.
- Adapter answers are copied to plain JSON data before they are validated, so subclasses and getters
  cannot pass the checks with one value and be used with another. Unpaired surrogates are refused, and
  cleaned on import.
- Model text in `spec.md` is fully Markdown-escaped. Mermaid labels have their own escaping.
- `audit` reports any unreadable file as exit 2, without a traceback. Zip and jsonl reading has total
  limits and streams members.
- Error text is bounded, never crashes the run, and has key-style secrets and bearer tokens redacted.
- Audit output escapes newlines, bidirectional overrides and surrogates inside model text.
- A source cited when the run allows none is flagged.
- Imported decisions are dropped when no merge decided anything.
- Duplicate phases are refused.
- Exports write all files or none, and nothing is overwritten.
- `--adapter` must name an LLMAdapter class, checked before it is called.
- Methodology rows are written only for matching phase titles.
- `check_council` checks every field's type.
- The JSON schemas ship in the package.
- Node port: the same fixes, plus `__proto__` keys kept as data.
- An unknown key holding an unpaired surrogate is shown as U+FFFD in error text, so the recorded run
  stays encodable (found while writing the Java and C# ports).
- PHP port: `-0.0` keeps its sign, and integers beyond 64 bits keep their exact digits (found by
  random comparison with Python).
