# Porting SmartSavant

A port is the engine, the gates, the schema checks, the exporters, the importer and the audit, in
another language. It runs in-process with the language's own adapters (an Electron or Node app calling
its own model endpoint, for example). Read `SEMANTICS.md` first. The vectors in `conformance/` are the
test.

## What a port contains

1. Canonical JSON and SHA-256 (`canonical.json`).
2. `figures`, `squash` and the gates (`figures.json`, `gates.json`).
3. `check_view`, `check_merge` with the exact error strings (`schema.json`).
4. The engine, the stub adapter and the scripted adapter (`runs.json`: council, digest, health).
5. The exporters: stories, spec Markdown, Mermaid, trainset (`runs.json`).
6. The importer and the audit (`audit.json`, plus `runs.json` audit).
7. A conformance runner (`<runner> <vectors dir>`) that prints failures and a last line
   `<port>: N/N vectors passed`, and exits 1 on any failure. N must equal the Python count.
8. An adapter interface matching `bindings/council.d.ts` (TypeScript). It may be async in languages
   where model calls are.

## How each file is checked (exactly what the Python runner does)

| File | Checks |
|---|---|
| `canonical.json` | `[canonical(input), sha256(canonical)]` |
| `figures.json` | `figures(text)` |
| `gates.json` | `flag_unsourced(view, allowed)`, plus `is_generic`, `is_fallback`, `has_placeholder_code`, `compliance_block`, `squash` on `text`, and `synthesis_ok(merge)` |
| `schema.json` | `check_view(input)` and `check_merge(input)` error lists |
| `runs.json` | Build `Council(idea, ScriptedAdapter(script), roster, phases, sources, max_loops, retries)` (null roster or phases mean the defaults) and run it. Then compare each `expect` field: council, digest, health, stories, spec_md, mermaid, trainset, trainset_all, audit. One check per field. |
| `audit.json` | import, and audit of councils, stories and trainset rows |

Comparisons are of JSON values: key order does not matter, integers compare as integers, `true` is not
`1`.

## Pitfalls

- **JavaScript `\s`** includes Unicode spaces. Use `[ \t\n\r\f\v]` where SEMANTICS says `\s`. `\w` and
  `\b` are ASCII in JS already. Do not use the `u` flag with `i` (it folds non-ASCII).
- **Lengths and slicing** count code points. JS: `[...s].length` and `[...s].slice(0, 120).join('')`.
- **Python's `str.split()`** is not the contract; `squash` is.
- **`dict.fromkeys` order** in the stub's `merge`: first occurrence wins.
- **Exception names in errors.** The scripted adapter raises `AdapterError`. The engine writes
  `"AdapterError: <message>"`.
- **JSON in trainset rows** uses sorted keys (recursively), `", "` and `": "` separators (Python's
  `json.dumps` defaults), and no `\u` escapes for non-ASCII. The spec Markdown and stories are plain
  strings.
- **Audit examples** are cut to 200 code points, at most 5 per finding.
- **Unpaired surrogates.** Python strings can hold them; the contract refuses them in answers and inputs,
  and turns them into U+FFFD on import and in unknown-key error text.
  - UTF-16 languages (JavaScript, Java, C#): replace each lone surrogate one character at a time, while
    dropping control characters. Replacing after the drop can join two lone halves into a valid pair.
  - Go and Rust strings cannot hold them. Those ports map a `\udXXX` escape in JSON to a code point
    in U+10F800-U+10FFFF so that it can still be detected; their READMEs give the trade-off.
- **Numbers.** `-0.0` keeps its sign, and integers beyond 64 bits keep their exact digits (the PHP port
  keeps them as a `BigInt` value).
- **Recursion.** Nesting is limited (32 in answers, 20 in imported text). A runtime with a small stack
  (Ruby) may need a lower JSON reader depth; its README says so.

## The existing ports

`ports/node`, `ports/go`, `ports/rust`, `ports/java`, `ports/csharp`, `ports/php` and `ports/ruby`. Each
README lists its files, how to run it, its documented differences from Python and what was tested.
