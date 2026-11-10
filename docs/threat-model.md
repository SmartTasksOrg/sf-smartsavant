# Threat model

SmartSavant runs model calls through an adapter and turns the answers into documents that people build
from and data that models train on. The risks are bad content becoming "decisions", and model text
doing something it should not.

## What is defended

- **Malformed or hostile adapter answers.** Every answer is first copied to plain JSON data. Subclasses
  of dict, str or list that lie in their methods are refused, as are other types, nesting deeper than
  32 levels and more than 1,000,000 characters of text. The copy is then checked against the contract:
  - exact keys, types and lengths (8000 code points; 20 items of 2000);
  - source id format;
  - no NUL characters and no unpaired surrogates.

  An invalid answer is retried and then recorded as `failed`, and its content is not used.
- **Fallback text presented as a decision.** Fallback text and generic steps never count as a full
  synthesis (`synthesis_ok`). Such phases become blocked stories and NOT DECIDED sections, and the
  audit reports them.
- **Learning from bad data.** The trainset skips:
  - synthetic runs;
  - merges that are not full;
  - views with unsourced figures.

  The audit finds contaminated rows in existing trainsets.
- **Adapters changing the run.** Every call gets deep copies of the context and the views.
- **Markup injection into the spec.**
  - Model text in `spec.md` is collapsed to one line, with `&`, `<` and `>` as entities and every
    Markdown metacharacter backslash-escaped. It cannot start a heading, list, code fence, link, image
    or table cell.
  - Mermaid labels are quoted and cut to 80 characters.
  - The CLI shows control characters, bidirectional overrides and unpaired surrogates escaped, and
    newlines inside model text too. Model text cannot redraw the screen or forge an output line.
- **Secrets in error text.** Adapter error messages are recorded with `key=...` style values and bearer
  tokens redacted (`gates.redact`) and cut to 500 characters. This is a pattern match: keep secrets out
  of your error messages anyway.
- **Overwriting files.** `run`, `export` and `import` refuse to overwrite. They write everything or
  nothing: each file goes to a temporary file first and is then linked into place.
- **Zip archives in audits.**
  - Only `*.jsonl` members are read, at most 1000 of them.
  - Each member is at most 64 MiB uncompressed. In all, at most 256 MiB are read and 200,000 rows
    parsed, and lines longer than 1 MiB count as unreadable.
  - Members are streamed and nothing is extracted to disk.
  - A damaged, encrypted or unsupported zip is reported as unreadable (exit 2).

## What is not defended

- **Prompt injection in the idea or the sources.** They reach your adapter's prompt. Validation limits
  the shape of what comes back, not its truth. Mark untrusted data in your prompts (docs/adapters.md),
  and treat every council output as a draft for people to review.
- **`--adapter module:Class` runs Python code** from your path, so use only your own adapters. The
  class must be an LLMAdapter subclass; that is checked before anything is called. The
  same goes for `ReplayAdapter` files: they are data, but untrusted council files should be audited,
  not replayed into production.
- **The gates are heuristics.**
  - A confident, wrong decision passes them.
  - A figure with a fake source id passes `flag_unsourced` when the run allows that id.
  - The compliance gate is a keyword stub.
- **Secrets.** The package never reads or stores model keys; your adapter does. Do not put keys in
  `--adapter-arg` or in council files.
- **Large inputs.** There is no limit on the number of phases, roster size or loops beyond the argument
  checks (`max_loops` up to 10). Each adapter call is yours to bound in time.
