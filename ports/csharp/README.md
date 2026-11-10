# SmartSavant for .NET

This is the SmartSavant council engine in C#. It contains the parts of `spec/SEMANTICS.md` that the shared
vectors test: canonical JSON and SHA-256, the gates, the schema checks (with `Plain()`), the engine, the
stub, scripted and replay adapters and the adapter interface, the exporters (stories, spec Markdown,
Mermaid, trainset), the SmartSignal importer and the audit. A .NET application runs it in-process and
supplies its own adapter, which calls its own model endpoint. The command line, the demos, zip trainsets
and the file writers stay in the Python reference.

| Project | What it is |
|---|---|
| `SmartSavant` | the library (assembly `SfSmartSavant`, namespace `SmartSavant`), net8.0 |
| `Conformance` | runs the shared vectors in `spec/conformance` |
| `Selftest` | checks the vectors do not cover: a hand-written adapter, exception text, answers that are not plain data, replay, refused inputs, Python float and string formatting, ASCII regex semantics |

| File in `SmartSavant/` | What it is |
|---|---|
| `Json.cs` | `JObj` (an object that keeps insertion order), the JSON reader (`long` / `BigInteger` / `double` numbers, lone surrogates kept), `Same()` value equality |
| `Py.cs` | the Python behaviour the reference relies on: code-point lengths and slices, code-point string order, `str()`, `repr()`, `repr(float)`, truthiness, `dict.get()`, `str.lower()`, `json.dumps()` |
| `Canonical.cs` | canonical JSON (`Dumps`, `Encode`), `Sha256Hex` |
| `Gates.cs` | `Squash`, `Blank`, `Redact`, `Figures`, `FlagUnsourced`, `IsGeneric`, `IsFallback`, `HasPlaceholderCode`, `SynthesisOk`, `ComplianceBlock` (`Rule`, `DefaultRules`) |
| `Schema.cs` | `Plain` (`NotPlainData`), `CheckView`, `CheckMerge`, `NormalizeView`, `CheckCouncil` |
| `Model.cs` | constructors for the view, merge, round, node and result records; `ResultFromDict` |
| `Council.cs` | the engine (`Run()`), `Health`, `CheckPhases`, `LoadPhases`, `ErrorText`, `DefaultRoster`, `DefaultPhases` |
| `Adapters.cs` | `ILLMAdapter`, `StubAdapter`, `ScriptedAdapter` (`AdapterError`), `ReplayAdapter` (`ReplayError`) |
| `Exporters.cs` | `RunDigest`, `ToStories`, `ToSpecMd`, `ToTrainset`, `ToMermaid`, `Md` |
| `Importer.cs` | `FromSmartsignal`, `Detect`, `IdeaText`, `Text` |
| `Audit.cs` | `AuditResult`, `AuditStories`, `AuditTrainset`, `ReadJsonl`, `AuditPath` (`.json` and `.jsonl`) |

There are no package dependencies: `nuget.config` clears every package source, so a restore never pulls
code from a registry. It uses only the base class library (System.Security.Cryptography for SHA-256,
System.Text.RegularExpressions, System.Numerics). JSON is read by the port's own reader, because
System.Text.Json does not keep unpaired surrogates and does not tell `1` from `1.0`. It is not published
on NuGet (`IsPackable` is false); reference the project from a clone.

## How to run

```sh
cd ports/csharp
dotnet run -c Release --project Conformance -- ../../spec/conformance
# csharp: 198/198 vectors passed
dotnet run -c Release --project Selftest
# csharp: selftest passed
```

From the repository root, `ports/conformance/run.sh csharp` runs the vectors through the shared harness.
All projects build with `TreatWarningsAsErrors`.

## Using it

Values are JSON data: `JObj` (keys in insertion order), `List<object?>`, `string`, `long`, `bool` and
null. The result of a run and everything the exporters return are such values, with the fields in the
order of `bindings/council.d.ts`.

```xml
<ProjectReference Include="../sf-smartsavant/ports/csharp/SmartSavant/SmartSavant.csproj" />
```

```csharp
using SmartSavant;

var council = new Council("Learn reusable automations from recorded operator sessions", new StubAdapter(),
    roster: new[] { "Developer", "UX/UI", "Product Owner" },          // null: the four of DefaultRoster
    phases: new[] { ("frame", "Frame", "What is the problem?"),
                    ("build", "Build", "What is the technical approach?") },   // null: DefaultPhases
    sources: new[] { "survey-2026" },                                 // source ids views may cite
    maxLoops: 2,                                                      // 1 to 10, default 1
    retries: 1);                                                      // 0 to 5, default 1
JObj res = council.Run();

Council.Health(res);                 // counts; "decided" is true only when every phase has a full synthesis
Exporters.ToSpecMd(res);             // Markdown; undecided phases are marked NOT DECIDED
Exporters.ToStories(res, "PROJ");    // ready or blocked stories
Exporters.ToTrainset(res);           // methodology rows only for a synthetic run such as this one
Audit.AuditResult(res);              // verdict "not build-ready": a stub run is synthetic
string councilJson = Py.JsonDumps(res, false);   // a council.json that the Python reference reads
```

A phase can also be a `[step_id, title, question]` list or a `{step_id, title, question}` `JObj`. Phases
are checked (`Council.CheckPhases`): three non-blank strings each and unique step ids. The idea, roster
names, sources and phase strings must not contain unpaired UTF-16 surrogates; the constructor throws
`ArgumentException` otherwise, with the Python reference's message. `Council.LoadPhases(path)` reads a
phases file in the format of `spec/phases.json`. `Audit.AuditPath(path)` audits a council JSON (this
package or SmartSignal), a stories JSON or a `.jsonl` trainset.

## Writing an adapter

An adapter implements `ILLMAdapter`: `Name` (default `"adapter"`), `Synthetic` (default false; true only
for stubs and demos), and two methods:

```csharp
object? View(string stakeholder, string role, string phaseId, string question, JObj context);
object? Merge(string phaseId, string question, List<JObj> views, JObj context);
```

The engine:

- calls them one at a time, in roster order, so an adapter does not need to be thread-safe;
- gives each call a fresh deep copy of its arguments (`context` is `{idea, iteration, sources, prior,
  locked}`; `views` are the phase's valid views as records);
- first copies every answer to plain data (`Schema.Plain`), then validates the copy with `CheckView` or
  `CheckMerge` and uses only the copy;
- retries a call that throws or returns an invalid answer;
- records a call that never succeeds as `failed` with its error, for example
  `"HttpRequestException: Connection refused"` or `"invalid: missing keys: sources"`. Each validation
  problem is cut to 200 code points and the whole error to 500.

The calls are synchronous. An adapter that calls a model over HTTP waits for the answer; the engine makes
one call at a time, so blocking on it is safe outside a single-threaded synchronization context.

`Plain` accepts null, `bool`, the integer types (`long`, `int`, ...) and `BigInteger`, `double`, `float`,
`string`, `JObj`, any `IDictionary` with `string` keys and any `IList`; the copy holds `long`, `double`,
`List<object?>` and `JObj`. It refuses, as `"invalid: <message>"`: nesting deeper than 32 levels
(`nested deeper than 32 levels`), more than 1,000,000 code points of text including keys (`more than
1,000,000 characters of text`), a key that is not a string (`object keys must be strings`) and any other
type (`<type name> is not JSON data`, for example `DateTime is not JSON data`).

Errors are recorded as `"<exception type name>: <message>"`. Unpaired surrogates in the message become
`?`, `key=VALUE`-style secrets and bearer tokens become `***` (`Gates.Redact`), and the text is cut to 500
code points. Note that .NET fills in a default message for exceptions created without one.

The answer formats are in `bindings/council.d.ts`, `spec/schema/view.schema.json` and
`spec/schema/merge.schema.json`; the prompt contracts are in `spec/prompts/`. The idea, the sources and the
earlier decisions are untrusted text, so keep them out of your instructions and pass them as data.

The example below posts to a model endpoint of your own. **The URL is an example placeholder**; replace it
with your endpoint and adapt the request and response handling to your API.

```csharp
using System.Net.Http.Headers;
using System.Text;
using SmartSavant;

sealed class MyEndpointAdapter : ILLMAdapter
{
    // EXAMPLE ONLY: replace with your own model endpoint.
    private static readonly Uri Endpoint = new("https://llm.example.invalid/v1/council");
    private readonly HttpClient http = new() { Timeout = TimeSpan.FromSeconds(60) };

    public MyEndpointAdapter(string token) =>
        http.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", token);

    public string Name => "my-endpoint";

    private object? Call(string task, JObj payload)
    {
        payload["task"] = task;
        var body = new StringContent(Py.JsonDumps(payload, false), Encoding.UTF8, "application/json");
        using var r = http.PostAsync(Endpoint, body).GetAwaiter().GetResult();
        if (!r.IsSuccessStatusCode) throw new HttpRequestException($"HTTP {(int)r.StatusCode}");  // recorded, then retried
        return Json.Parse(r.Content.ReadAsStringAsync().GetAwaiter().GetResult());               // the answer object itself
    }

    public object? View(string stakeholder, string role, string phaseId, string question, JObj context) =>
        Call("view", Py.O("stakeholder", stakeholder, "role", role, "phase_id", phaseId, "question", question, "context", context));

    public object? Merge(string phaseId, string question, List<JObj> views, JObj context) =>
        Call("merge", Py.O("phase_id", phaseId, "question", question, "views", views.Cast<object?>().ToList(), "context", context));
}
```

## What was tested

The .NET SDK 8.0.131 (runtime 8.0.31) on Linux x86_64, with the Python reference on Python 3.13.16:

- the 198 vectors (about 0.25 s from a Release build);
- `Selftest`;
- a differential run against the Python reference: about 70,000 random cases in the vector formats
  (random strings through the gates; random views and merges, with wrong types, long strings, control
  characters, NUL and unpaired surrogates, through `CheckView` and `CheckMerge`; random JSON through the
  canonical writer; about 600 random councils with scripted failures, raises, invalid answers, retries,
  loops and sources through every exporter and the audit; random SmartSignal exports with wrong types,
  floats such as `"iteration": 1.0`, `__proto__` keys and deep nesting through the importer and the audit;
  random stories and trainset rows). No difference was found. The generator script is not part of the
  repository;
- `Py.Num` (Python's `repr(float)`) against Python on about 200,000 random doubles: no difference.

## Limits and differences

- **Regular expressions.** .NET's `\s`, `\w`, `\d` and `\b` match Unicode characters, and
  `RegexOptions.IgnoreCase` matches some non-ASCII letters (U+212A KELVIN SIGN matches `k`). The gates
  therefore spell out the ASCII classes of SEMANTICS.md section 2 and make patterns case-insensitive with
  explicit `[xX]` classes (`Gates.Ci`); no pattern uses `IgnoreCase`.
- `Py.Lower` (used by `ComplianceBlock` and the trainset's methodology rows) is `ToLowerInvariant` with
  U+0130 written as `i` plus U+0307, as Python does. It does not apply Python's final-sigma rule and uses
  .NET's tables, which can differ from Python's for recently assigned characters. The default compliance
  rules use ASCII words only, so they are not affected. The same holds for the printable test of `repr()`.
- The JSON reader accepts at most 512 levels of nesting; Python's `json` is bounded by the interpreter's
  recursion limit instead, so a file nested between those depths is read by one and refused by the other.
- The stub's tag hashes the UTF-8 of the idea. `Council` refuses an idea with an unpaired surrogate, as
  Python does, but a direct call of `StubAdapter.View` with one hashes it with U+FFFD; Python raises.
- `AuditPath` reads `.json` and `.jsonl` files only (with the row, size and line limits of Python); zip
  trainsets are read by the Python reference.
- The command line, the demos, `load_adapter`, `write_all`, `write_new` and the terminal rendering of audit
  reports are not ported.
- `Council` is not thread-safe; use one instance per run.
