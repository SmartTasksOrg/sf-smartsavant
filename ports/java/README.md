# SmartSavant for Java

This is the SmartSavant council engine in Java. It contains the parts of `spec/SEMANTICS.md` that the
shared vectors test: canonical JSON and SHA-256, the gates, the schema checks (with `plain()`), the engine,
the stub, scripted and replay adapters and the adapter interface, the exporters (stories, spec Markdown,
Mermaid, trainset), the SmartSignal importer and the audit. A Java application runs it in-process and
supplies its own adapter, which calls its own model endpoint. The command line, the demos, zip trainsets
and the file writers stay in the Python reference.

It uses only the JDK and has no dependencies. It compiles with `--release 17` and was tested with
OpenJDK 21.0.12.1. It is not published to Maven Central or any other repository. Use it from a clone of
this repository.

## Layout

All classes are in `src/main/java/cloud/smarttasks/smartsavant/`.

| File | What |
|---|---|
| `Json.java` | JSON reader (ordered objects, `Long` / `BigInteger` / `Double` numbers, lone surrogates kept), `same()` value equality |
| `Py.java` | the Python behaviour the reference relies on: code-point lengths and slices, code-point string order, `str()`, `repr()`, `repr(float)`, truthiness, `dict.get()`, `str.lower()`, `json.dumps()` |
| `Canonical.java` | canonical JSON (`dumps`, `encode`), `sha256Hex` |
| `Gates.java` | `squash`, `blank`, `redact`, `figures`, `flagUnsourced`, `isGeneric`, `isFallback`, `hasPlaceholderCode`, `synthesisOk`, `complianceBlock` (`Rule`, `DEFAULT_RULES`) |
| `Schema.java` | `plain` (`NotPlainData`), `checkView`, `checkMerge`, `normalizeView`, `checkCouncil` |
| `Model.java` | constructors for the view, merge, round, node and result records; `resultFromDict` |
| `Council.java` | the engine (`run()`), `health`, `checkPhases`, `loadPhases`, `errorText`, `DEFAULT_ROSTER`, `DEFAULT_PHASES` |
| `LLMAdapter.java` | the adapter interface |
| `StubAdapter.java`, `ScriptedAdapter.java` (`AdapterError`), `ReplayAdapter.java` (`ReplayError`) | the adapters |
| `Exporters.java` | `runDigest`, `toStories`, `toSpecMd`, `toTrainset`, `toMermaid`, `md` |
| `Importer.java` | `fromSmartsignal`, `detect`, `ideaText`, `text` |
| `Audit.java` | `auditResult`, `auditStories`, `auditTrainset`, `readJsonl`, `auditPath` (`.json` and `.jsonl`) |
| `Conformance.java` | the vector runner |
| `Selftest.java` | checks the vectors do not cover: a hand-written adapter, exception text, answers that are not plain data, replay, refused inputs, Python float and string formatting, ASCII regex semantics |
| `build.sh` | builds into `build/classes` without Maven |
| `pom.xml` | the same build for Maven users (no dependencies) |

## How to run

From the repository root:

```sh
ports/java/build.sh
java -cp ports/java/build/classes cloud.smarttasks.smartsavant.Conformance spec/conformance
# last line: java: 198/198 vectors passed
java -cp ports/java/build/classes cloud.smarttasks.smartsavant.Selftest
# java: selftest passed
ports/conformance/run.sh java        # the vectors through the shared harness
```

`build.sh` runs `javac -Xlint:all,-serial -Werror --release 17` into `ports/java/build/classes`. The
`pom.xml` (`mvn -q compile`, output in `target/`) is not used by this repository's checks. It was not run
here: Maven 3.9.11 is installed, but the Maven build plugins could not be downloaded from the test
machine.

## Using it

Values are JSON data: `Map<String, Object>` (a `LinkedHashMap` in insertion order), `List<Object>`,
`String`, `Long`, `Boolean` and null. The result of a run and everything the exporters return are such
values, with the fields in the order of `bindings/council.d.ts`.

```java
import cloud.smarttasks.smartsavant.*;
import java.util.List;
import java.util.Map;

Council council = new Council("Learn reusable automations from recorded operator sessions", new StubAdapter(),
    List.of("Developer", "UX/UI", "Product Owner"),                     // null: the four of DEFAULT_ROSTER
    List.of(List.of("frame", "Frame", "What is the problem?"),
            List.of("build", "Build", "What is the technical approach?")),  // null: DEFAULT_PHASES
    List.of("survey-2026"),                                             // source ids views may cite
    2,                                                                  // max_loops, 1 to 10
    1);                                                                 // retries, 0 to 5
Map<String, Object> res = council.run();

Council.health(res);                 // counts; "decided" is true only when every phase has a full synthesis
Exporters.toSpecMd(res);             // Markdown; undecided phases are marked NOT DECIDED
Exporters.toStories(res, "PROJ");    // ready or blocked stories
Exporters.toTrainset(res);           // methodology rows only for a synthetic run such as this one
Audit.auditResult(res);              // verdict "not build-ready": a stub run is synthetic
String councilJson = Py.jsonDumps(res, false);   // a council.json that the Python reference reads
```

`new Council(idea, adapter)` uses the defaults. A phase can also be a `{step_id, title, question}` map.
Phases are checked (`Council.checkPhases`): three non-blank strings each and unique step ids. The idea,
roster names, sources and phase strings must not contain unpaired UTF-16 surrogates; the constructor throws
`IllegalArgumentException` otherwise, with the Python reference's message. `Council.loadPhases(path)`
reads a phases file in the format of `spec/phases.json`. `Audit.auditPath(path)` audits a council JSON
(this package or SmartSignal), a stories JSON or a `.jsonl` trainset.

## Writing an adapter

An adapter implements `LLMAdapter`: `name()` (default `"adapter"`), `synthetic()` (default false; true only
for stubs and demos), and two methods:

```java
Object view(String stakeholder, String role, String phaseId, String question, Map<String, Object> context) throws Exception;
Object merge(String phaseId, String question, List<Map<String, Object>> views, Map<String, Object> context) throws Exception;
```

The engine:

- calls them one at a time, in roster order, so an adapter does not need to be thread-safe;
- gives each call a fresh deep copy of its arguments (`context` is `{idea, iteration, sources, prior,
  locked}`; `views` are the phase's valid views as maps);
- first copies every answer to plain data (`Schema.plain`), then validates the copy with `checkView` or
  `checkMerge` and uses only the copy;
- retries a call that throws or returns an invalid answer;
- records a call that never succeeds as `failed` with its error, for example
  `"IOException: connection reset"` or `"invalid: missing keys: sources"`. Each validation problem is cut
  to 200 code points and the whole error to 500.

`plain` accepts null, `Boolean`, `Long`, `Integer`, `Short`, `Byte`, `BigInteger`, `Double`, `Float`,
`String`, any `List` and any `Map` with `String` keys; the copy holds `Long`, `Double`, `ArrayList` and
`LinkedHashMap`. It refuses, as `"invalid: <message>"`: nesting deeper than 32 levels (`nested deeper than
32 levels`), more than 1,000,000 code points of text including keys (`more than 1,000,000 characters of
text`), a key that is not a string (`object keys must be strings`) and any other type (`<simple class
name> is not JSON data`, for example `Date is not JSON data`).

Errors are recorded as `"<simple class name>: <message>"` (an exception without a message gives
`"IllegalStateException: "`, as Python's `str()` of an exception without arguments). Unpaired surrogates
in the message become `?`, `key=VALUE`-style secrets and bearer tokens become `***` (`Gates.redact`), and
the text is cut to 500 code points. Only `Exception` is caught; an `Error` such as `OutOfMemoryError`
ends the run.

The answer formats are in `bindings/council.d.ts`, `spec/schema/view.schema.json` and
`spec/schema/merge.schema.json`; the prompt contracts are in `spec/prompts/`. The idea, the sources and the
earlier decisions are untrusted text, so keep them out of your instructions and pass them as data.

The example below posts to a model endpoint of your own. **The URL is an example placeholder**; replace it
with your endpoint and adapt the request and response handling to your API.

```java
import java.net.URI;
import java.net.http.*;
import java.time.Duration;

final class MyEndpointAdapter implements LLMAdapter {
    // EXAMPLE ONLY: replace with your own model endpoint.
    private static final URI ENDPOINT = URI.create("https://llm.example.invalid/v1/council");
    private final HttpClient http = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(10)).build();
    private final String token;

    MyEndpointAdapter(String token) { this.token = token; }

    @Override public String name() { return "my-endpoint"; }

    private Object call(String task, Map<String, Object> payload) throws Exception {
        payload.put("task", task);
        HttpRequest req = HttpRequest.newBuilder(ENDPOINT).timeout(Duration.ofSeconds(60))
            .header("content-type", "application/json").header("authorization", "Bearer " + token)
            .POST(HttpRequest.BodyPublishers.ofString(Py.jsonDumps(payload, false))).build();
        HttpResponse<String> r = http.send(req, HttpResponse.BodyHandlers.ofString());
        if (r.statusCode() != 200) throw new IllegalStateException("HTTP " + r.statusCode());  // recorded, then retried
        return Json.parse(r.body());                                                           // the answer object itself
    }

    @Override
    public Object view(String stakeholder, String role, String phaseId, String question, Map<String, Object> context) throws Exception {
        return call("view", Py.obj("stakeholder", stakeholder, "role", role, "phase_id", phaseId, "question", question, "context", context));
    }

    @Override
    public Object merge(String phaseId, String question, List<Map<String, Object>> views, Map<String, Object> context) throws Exception {
        return call("merge", Py.obj("phase_id", phaseId, "question", question, "views", views, "context", context));
    }
}
```

## What was tested

OpenJDK 21.0.12.1 on Linux x86_64, with the Python reference on Python 3.13.16:

- the 198 vectors (about 0.4 s including JVM start);
- `Selftest`;
- a differential run against the Python reference: about 70,000 random cases in the vector formats
  (random strings through the gates; random views and merges, with wrong types, long strings, control
  characters, NUL and unpaired surrogates, through `checkView` and `checkMerge`; random JSON through the
  canonical writer; about 600 random councils with scripted failures, raises, invalid answers, retries,
  loops and sources through every exporter and the audit; random SmartSignal exports with wrong types,
  floats such as `"iteration": 1.0`, `__proto__` keys and deep nesting through the importer and the audit;
  random stories and trainset rows). No difference was found. The generator script is not part of the
  repository;
- `Py.num` (Python's `repr(float)`) against Python on about 200,000 random doubles: no difference.

## Limits and differences

- `Py.num` relies on `Double.toString` giving the shortest round-trip digits, which is true from JDK 19.
  On JDK 17 and 18 a float can print with more digits than Python's `repr()`. Floats appear only where the
  importer or the audit turn a number from an imported file into text.
- Whether a character is printable in `repr()` (rare fallbacks of the audit and the spec) and the
  lower-casing in `complianceBlock` come from the JDK's Unicode tables, which can differ from Python's for
  recently assigned characters. The default compliance rules use ASCII words only.
- The JSON reader accepts at most 512 levels of nesting; Python's `json` is bounded by the interpreter's
  recursion limit instead, so a file nested between those depths is read by one and refused by the other.
- The stub's tag hashes the UTF-8 of the idea. `Council` refuses an idea with an unpaired surrogate, as
  Python does, but a direct call of `StubAdapter.view` with one hashes it with `?`; Python raises.
- `auditPath` reads `.json` and `.jsonl` files only (with the row, size and line limits of Python); zip
  trainsets are read by the Python reference.
- The adapter interface is synchronous. The command line, the demos, `load_adapter`, `write_all`,
  `write_new` and the terminal rendering of audit reports are not ported.
- `Council` is not thread-safe; use one instance per run.
