# SmartSavant for Go

This is the SmartSavant council engine in Go. It contains the parts of `spec/SEMANTICS.md` that the
shared vectors test: canonical JSON and SHA-256, the gates, the schema checks (with `Plain` and the exact
error strings), the engine, the stub, scripted and replay adapters, the exporters (stories, spec Markdown,
Mermaid, trainset), the SmartSignal importer and the audit. A Go program runs it in-process and supplies
its own adapter, which calls its own model. The command line, the demos, zip trainsets and the file
writers stay in the Python reference.

It uses only the Go standard library and has no dependencies. Module path:
`github.com/SmartTasksOrg/sf-smartsavant/ports/go`, package `smartsavant`. It is not published on any
registry or module proxy. Use it from a clone, for example with a `replace` directive that points at
`ports/go`.

## Layout

| Path | What |
|---|---|
| `smartsavant/json.go` | `Value`, `Object` (keeps key order), `ParseJSON`, `Canonical`, `SHA256Hex`, `PyDumps` (Python's `json.dumps`), `Equal` |
| `smartsavant/py.go` | the Python behaviour the reference relies on: code-point `Len` and `Head`, `Str`, `Repr`, `Truthy`, `str.lower()` |
| `smartsavant/gates.go` | `Squash`, `Blank`, `Redact`, `Figures`, `FlagUnsourced`, `IsGeneric`, `IsFallback`, `HasPlaceholderCode`, `SynthesisOK`, `ComplianceBlock` |
| `smartsavant/schema.go` | `Plain` (`*NotPlainData`), `CheckView`, `CheckMerge`, `NormalizeView`, `CheckCouncil` |
| `smartsavant/model.go` | `CouncilResult` and its parts, `Value()` (council.json form), `ResultFromValue` |
| `smartsavant/adapters.go` | `Adapter`, `Context`, `StubAdapter`, `ScriptedAdapter` (`*AdapterError`), `ReplayAdapter` (`*ReplayError`) |
| `smartsavant/engine.go` | `NewCouncil`, `Council.Run`, `Options`, `DefaultOptions`, `Phase`, `DefaultPhases`, `CheckPhases`, `LoadPhases`, `HealthOf`, `ErrorText` |
| `smartsavant/exporters.go` | `RunDigest`, `ToStories`, `ToSpecMd`, `ToTrainset`, `ToMermaid`, `Md`, `MermaidLabel` |
| `smartsavant/importer.go` | `FromSmartSignal`, `Detect`, `IdeaText` |
| `smartsavant/audit.go` | `AuditResult`, `AuditStories`, `AuditTrainset`, `ReadJSONL`, `AuditPath` (`.json` and `.jsonl`) |
| `smartsavant/conformance.go` | `RunConformance`, shared by the runner and the tests |
| `smartsavant/smartsavant_test.go` | the vectors, plus error text, panics, `Plain` limits, unpaired surrogates, replay |
| `cmd/conformance` | the vector runner |

## How to run

```sh
cd ports/go
go vet ./... && go run ./cmd/conformance ../../spec/conformance   # last line: go: 198/198 vectors passed
go test ./...
cd ../..
ports/conformance/run.sh go
```

## Using it

```go
import sv "github.com/SmartTasksOrg/sf-smartsavant/ports/go/smartsavant"

opts := sv.DefaultOptions()                       // default roster and phases, MaxLoops 1, Retries 1
opts.Roster = []string{"Developer", "UX/UI", "Product Owner"}
opts.Phases = []sv.Phase{{StepID: "frame", Title: "Frame", Question: "What is the problem?"}}
opts.Sources = []string{"survey-2026"}            // source ids views may cite
council, err := sv.NewCouncil("Learn reusable automations from recorded operator sessions", sv.StubAdapter{}, opts)
if err != nil { ... }                             // a blank idea, duplicate names, MaxLoops outside 1..10 ...
res := council.Run()                              // *sv.CouncilResult; res.Value() is the council.json object

sv.HealthOf(res)                // counts; Decided is true only when every phase has a full synthesis
sv.ToSpecMd(res)                // Markdown; undecided phases are marked NOT DECIDED
sv.ToStories(res, "PROJ")       // ready or blocked stories
sv.ToTrainset(res, false)       // methodology rows only for a synthetic run such as this one
sv.AuditResult(res).Verdict     // "not build-ready": the stub is synthetic
sv.PyDumps(res.Value(), false)  // JSON text
```

`Options` has no usable zero value: a zero `MaxLoops` is an error, as in the reference. Start from
`DefaultOptions()`.

## Writing an adapter

An adapter implements four methods:

```go
type Adapter interface {
	Name() string                        // written into council.json
	Synthetic() bool                     // true for stubs and demos: never learned from or built on
	View(req ViewRequest) (any, error)   // req.Stakeholder, Role, PhaseID, Question, Context
	Merge(req MergeRequest) (any, error) // req.PhaseID, Question, Views (the valid views), Context
}
```

`View` returns a view answer and `Merge` a merge answer (`spec/schema/*.json`), as JSON data: `nil`,
`bool`, Go integers and floats, `string`, slices, `map[string]any` (copied in sorted key order) or
`*Object` (keeps its order). `ParseJSON` turns a model's JSON text into such a value. The engine copies
the answer with `Plain`, validates it with `CheckView` / `CheckMerge`, and retries an invalid answer or a
returned error `Retries` times; then it records the view as `failed` (or the merge as `failed`) with the
error text, and never invents content. `Context` (idea, iteration, sources, prior, locked) and `Views`
are fresh copies for every call.

The recorded error is `"<Name>: <message>"`, secrets redacted, at most 500 code points. `<Name>` is the
error's `ErrorName()` when it has that method (the scripted adapter's `*AdapterError` gives
`AdapterError`), otherwise its Go type name (`Error` for `errors.New` and `fmt.Errorf`). A panic in `View`
or `Merge` is recovered and recorded as `Panic: <value>`.

Treat the idea and the sources as untrusted data in your prompts; `spec/prompts/` has the contracts.

## What was tested

Go 1.24.7 on Linux x86-64 (the module declares `go 1.22`): the 198 vectors, `go vet`, `go test`, and a
differential comparison with the Python reference (Python 3.13.16): about 44,000 random strings through
`Figures`, `IsGeneric`, `IsFallback`, `HasPlaceholderCode`, `Squash`, `Redact`, `ComplianceBlock`, `Md`
and `MermaidLabel`, and eight sets of about 1,200 random vectors each (scripted runs with invalid
answers, errors, loops and sources; SmartSignal exports with values of every JSON type; stories;
trainset rows; view and merge checks). All matched. That generator is not part of the repository.

## Limits and differences

- **Unpaired surrogates.** Go strings hold UTF-8, which cannot encode U+D800-U+DFFF. `ParseJSON` maps
  an unpaired surrogate escape (`"\ud800"`) to the code point U+10F800 + (s - 0xD800), in the last 2048
  code points of plane 16 (private use). The checks find it there (`contains an unpaired surrogate`), the
  importer and the unknown-key error text replace it with U+FFFD, adapter error text with `?`, and the writers (`Canonical`, `PyDumps`, `Repr`)
  write it back as a `\udXXX` escape. Differences: a real character from U+10F800-U+10FFFF in the input
  is treated as an unpaired surrogate; Python writes a lone surrogate raw in `json.dumps` and cannot
  encode it in `canonical` at all; keys holding one sort after other characters here and before them in
  Python.
- `Repr` (used by `str()` of non-string values, e.g. the counts in the spec header) decides printability
  with Go's `unicode.IsPrint`, whose Unicode version may differ from Python's.
- `str.lower()` is `strings.ToLower` with U+0130 mapped to `i̇` as Python does; Python's final-sigma
  rule is not applied (no keyword of the compliance rules contains a sigma).
- Integers outside the int64 range are kept as `BigInt` (their decimal text).
- `ParseJSON` refuses nesting deeper than 1,000 levels and input that is not valid UTF-8.
- `Plain` refuses Go types that are not JSON data with `"<Go type> is not JSON data"`, where Python names
  the Python type.
- Not included: the command line, the demos, the file writers (`write_all`), `load_adapter`, and zip
  trainsets (`AuditPath` reads `.json` and `.jsonl`).
