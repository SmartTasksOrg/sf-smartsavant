# SmartSavant semantics

This is the contract for every engine: the Python reference (`src/sf_smartsavant`) and the ports in
`ports/`. The vectors in `spec/conformance/` test it. When this text and the vectors disagree, that is
a bug to report. The Python code is the detailed reference; this file says what must be identical.

## 1. Terms

- A **council** runs a list of **phases** (`spec/phases.json`: `step_id`, `title`, `question`) with a
  **roster** of stakeholder names. Each stakeholder gives a **view** per phase.
- A **juror** merges the valid views of a phase into a **merge**. That merge may recommend another
  iteration of the phase (up to `max_loops`).
- All model access goes through an **adapter** with two calls, `view` and `merge`. The engine never calls
  a model itself.
- A council refuses an idea, roster, phases or sources containing unpaired surrogates.
- Defaults:
  - roster: `Developer`, `UX/UI`, `Product Owner`, `Architecture/Infra`;
  - phases: frame, concept, design, build, integrate, measure, spec;
  - `max_loops` 1 (1 to 10), `retries` 1 (0 to 5).

## 2. Regular expressions and text

All patterns are ASCII patterns:
- `\s` means `[ \t\n\r\f\v]`;
- `\w` means `[A-Za-z0-9_]`;
- `\b` is the ASCII word boundary;
- case-insensitive matching folds ASCII letters only.

`squash(text)` replaces runs of `[ \t\n\r\f\v]` with one space and trims; no other character counts as
whitespace. **Blank** means empty or made only of `[ \t\n\r\f\v]`: every "non-empty" and "non-blank"
rule below uses this definition, so U+3000, U+00A0 or U+001C are content. Lengths are counted in Unicode
code points, never in UTF-16 units or bytes.

## 3. Adapter contract and validation

`view(stakeholder, role, phase_id, question, context)` returns an object with exactly these keys:

| Key | Rule |
|---|---|
| `w5h1` | an object with exactly `who`, `what`, `where`, `when`, `why`, `how`, each a string |
| `done_so_far`, `contribution` | strings |
| `devils_advocate` | a non-empty string (not only whitespace) |
| `missing` | a string, or a list of strings (a string is normalised to a one-item list; an empty or blank string to `[]`) |
| `sources` | a list of source ids matching `[A-Za-z0-9._:/#@-]{1,200}` |

`merge(phase_id, question, views, context)` returns an object with these keys:

| Key | Rule |
|---|---|
| `merge_summary` | a non-empty string |
| `decision` | a string |
| `resolved`, `open_issues`, `next_steps` | lists of strings |
| `reversals` | a list of strings (optional; `[]` when absent) |
| `spec_delta` | a string (limit 20000) |
| `loop_recommended` | a boolean |

Before any check, the answer is copied to plain JSON data (`plain()`):
- only exact objects, arrays, strings, numbers, booleans and null are accepted, with string keys;
- subclasses, getters and other types are refused;
- depth is at most 32;
- the strings, keys included, hold at most 1,000,000 code points in all.

The copy is what is validated and what is used. Its error messages are those of `schema.py`.

General limits:
- strings at most 8000 code points, without NUL and without unpaired surrogates (U+D800-U+DFFF);
- lists at most 20 items;
- each list item a non-empty, non-blank string of at most 2000 code points.

`check_view` and `check_merge` return the list of error messages in the order the Python reference
produces them. Unknown keys are reported as `unknown keys (N): ` followed by at most 5 keys:
- the keys are sorted;
- control characters (U+0000-U+001F, U+007F-U+009F) are dropped;
- unpaired surrogates become U+FFFD;
- each key is cut to 40 code points, plus `...` when cut. The exact strings are part of the contract, because they appear in `council.json`; the
`schema.json` vectors fix them. Unknown keys are an error.

`context` is a fresh deep copy for each call:
- `idea`;
- `iteration` (1-based);
- `sources` (the run's allowed ids);
- `prior` (`[{step, decision}]` of every earlier iteration, of this phase or an earlier one, that reached a full synthesis);
- `locked` (the locked decisions so far).

## 4. The engine

For each phase, iteration `it` from 1:

1. For each stakeholder in roster order, call `view`.
   - An exception, or an answer with errors, is retried; there are `1 + retries` attempts in all.
   - If no attempt is valid, the view is `{status: "failed", error}` with every other field empty.
   - For an exception, the error is `"<ExceptionName>: <message>"`:
     - the message is `"(message not printable)"` when it cannot be turned into a string;
     - secrets are redacted (`gates.redact`);
     - it is cut to 500 code points.
   - For an invalid answer, the error is `"invalid: "` plus the first five errors, each cut to 200 code
     points, joined with `"; "` and cut to 500.
   - The error of the last attempt is recorded.
   - The ports use `AdapterError` as the exception name of the scripted adapter.
2. Each valid view gets `status: "ok"` and its fields; `missing` is normalised. Gate flags (section 5)
   are appended to `flags` with `step_id` and `iteration` added.
3. If no view is ok, the juror is not called: `synthesis_status: "no_views"`, error
   `"no valid view to merge"`.
4. Otherwise call `merge` with the ok views as plain objects, in the order of the record fields.
   - Validation and retries work as for views; if it never becomes valid, the status is `failed` with
     the error.
   - A valid merge is `full` if `synthesis_ok` holds, otherwise `thin`.
5. Append the round `{step_id, step_title, iteration, question, views, juror}`. Also append the
   decision node `{step_id, iteration, branches: contributions of ok views, decision, next_steps}`.
6. If the merge is `full`:
   - append its `resolved` entries to `locked_decisions`, skipping entries already there;
   - append `{step, decision}` to `prior`.
7. Repeat the phase while `loop_recommended` is true and `it < max_loops`.

The result is
`{schema: "smartsavant.council/1", idea, adapter, synthetic, stakeholders, sources, rounds,
decision_tree, locked_decisions, flags}`.
- `adapter` is the adapter's `name`.
- `synthetic` is true for adapters whose output is not a real deliberation: the stub, the scripted
  adapter, and demos.

**Stub adapter**
- `tag` is the first 8 hex digits of sha256(UTF-8(`stakeholder|phase_id|idea`)), written as `sv` +
  tag (so that it never reads as a figure).
- `why` is the first 120 code points of the idea.
- The exact answers are in `adapters/stub.py`; the `runs.json` vectors fix them.

**Scripted adapter**
- Keys are `phase|iteration|stakeholder` for views and `phase|iteration` for merges.
- Each key holds a list of answers, one per call; the last answer repeats.
- An entry `{"raise": m}` raises `AdapterError(m)`.
- Keys that are not scripted get stub answers.

## 5. Gates

**`figures(text)`** returns numbers that state a quantity, in order of appearance. A match is:
- not preceded by `\w` or `.`;
- an optional currency `£ $ €` (with an optional space);
- a number:
  - `\d{1,3}(,\d{3})+(\.\d+)?`, or
  - `\d+\.\d+`, or
  - `\d+`;
- an optional space and unit from the list in `gates.py`;
- not followed by `\w`.

The match counts only if it has a currency, a unit, a comma or a decimal point. The figure is the
matched text, trimmed.

**`flag_unsourced(view, allowed)`**
- The text is `contribution`, `devils_advocate`, `done_so_far` and `missing` (a list joined with
  spaces), joined with spaces.
- If the view cites no source, each figure gives `{flag: "unsourced_figures", figure, stakeholder}`.
- Then each cited id not in `allowed` gives `{flag: "unknown_source", source, stakeholder}`. When
  `allowed` is empty, every cited id is unknown.
- Run-level sources never excuse a view that cites none.

**`is_generic(step)`** is true for:
- a non-string, empty or blank step;
- "advance the <words> with the decided approach";
- a single word followed by `X`, `Y` or `Z`;
- an optional "next step(s):" followed by `tbd`, `todo`, `n/a`, `none`, `proceed`, `continue` or
  nothing, with any trailing `.!?`;
- fallback text.

The exact patterns are in `gates.py`.

**`is_fallback(text)`** and **`has_placeholder_code(text)`**: the patterns are in `gates.py`.

**`synthesis_ok(merge)`** requires all of:
- the status is `full` (or no status is given);
- `merge_summary` and `decision` are non-blank;
- the summary, the decision, the resolved entries and the next steps contain no fallback text;
- `resolved` is non-empty;
- at least one next step is not generic.

**`compliance_block(text, rules)`**
- A rule is `(name, groups)`. It fires when the lowercased text contains a word from every group.
- The defaults are in `gates.py`.
- This is a keyword stub; a private build plugs in a real policy.

## 6. Exports

- **`run_digest`** is the lowercase hex sha256 of the canonical JSON of the whole result. Canonical JSON
  is as in SmartWatch: keys sorted by bytes, no whitespace, integers only, and only `"` `\` and controls
  below U+0020 escaped.
- **Stories**
  - There is one story per phase, taken from the phase's last iteration.
  - The story is `ready` when that merge is `full` and has a non-generic next step. Its title is the
    first non-generic step.
  - Otherwise it is `blocked`, with the title "Decide <title>: the council did not reach a decision
    (<status>)".
  - Field order and texts are as in `exporters.py`.
- **Spec (Markdown)**
  - Every piece of model text passes through `md()`:
    - `squash`;
    - the characters U+0000-U+001F, U+007F-U+009F, U+202A-U+202E and U+2066-U+2069 become U+FFFD;
    - `&`, `<` and `>` become `&amp;`, `&lt;` and `&gt;`;
    - each of ``\ ` * _ { } [ ] ( ) # + - . ! | ~`` gets a backslash before it.
  - Undecided phases are marked **NOT DECIDED**.
- **Trainset**
  - There is one methodology row per phase used, for default phase ids whose title starts with the id
    (case-insensitive).
  - Synthetic runs give nothing else, unless `allow_synthetic` is set.
  - Expert rows come from ok views with no flag in that phase and iteration; their instruction names the
    stakeholder.
  - Facilitation rows come from full merges.
  - JSON inside rows uses sorted keys and does not escape non-ASCII.
- **Mermaid**: labels are `squash`ed, with the same control and bidi characters as U+FFFD, cut to 80
  code points, with `&`, `"`, `<` and `>` replaced, then quoted (`_label` in `exporters.py`).

## 7. Imports and audit

**Import.** The importer reads the two SmartSignal shapes described in `importer.py`. It is
type-robust:
- values of the wrong type are skipped or read as empty;
- text deeper than 20 levels reads `[nested too deep]`;
- unpaired surrogates become U+FFFD;
- the `decisions` list is used only when a merge is full, and without fallback entries.

The mapping:
- `source: "offline"` maps to `offline`;
- a `full` merge that fails `synthesis_ok` becomes `thin`;
- object contributions are flattened to `key: value` lines;
- repeated `(step_id, iteration)` pairs get `.2`, `.3` and so on.

**Audit.** The codes and severities are listed in `audit.py`.
- A merge recorded as `full` that fails `synthesis_ok` counts as not decided.
- The adapters `stub`, `scripted` and `demo*` count as synthetic.
- Trainset rows that are not objects with an `output` string or a `messages` list are
  AUDIT-TRAINSET-UNREADABLE.
- `summary.kinds` is a list of `[kind, count]` pairs, sorted by kind. Findings sort by severity (error, warning,
info), then by code; there are at most 5 examples, each at most 200 code points. The verdict is
`build-ready` only without errors. A synthetic run is an error: it is never a basis for building.

## 8. Not covered by the vectors

Real adapters (they call models), the command line, and file reading (zip, jsonl). The ports implement
the engine, the gates, the exports, the import and the audit on in-memory data.
