# SmartSavant for PHP

This is the PHP port of the SmartSavant council engine. It implements the parts of `spec/SEMANTICS.md`
that the shared vectors test:

- canonical JSON and SHA-256
- the gates: `figures`, `squash`, `blank`, `flagUnsourced`, `isGeneric`, `isFallback`, `hasPlaceholderCode`,
  `synthesisOk`, `complianceBlock`, `redact`
- the schema checks: `plain`, `checkView`, `checkMerge` (with the exact error strings), `normalizeView`,
  `checkCouncil`
- the engine: `Council` with retries, statuses, loops, locked decisions and prior decisions
- the stub, scripted and replay adapters, and the `LLMAdapter` base class
- the exporters: stories, spec Markdown, Mermaid, trainset
- the SmartSignal importer and the audit

A PHP application runs it in-process and supplies its own adapter, which calls its own model endpoint. The
command line, the demos, zip trainsets and the file writers stay in the Python reference.

It needs PHP 8.1 or newer with the `mbstring` and `pcre` extensions. It was tested with PHP 8.3.6. It uses
no Composer packages. It does not use `ext-json`: it has its own JSON reader, so that unpaired surrogates
and the difference between `1` and `1.0` are kept as Python keeps them.

It is not published on Packagist. Use it from a clone of this repository with
`require 'ports/php/src/SmartSavant.php';`. You can also add the clone as a Composer `path` repository
(`smarttasksorg/sf-smartsavant`).

## Layout

| Path | Contents |
|---|---|
| `src/SmartSavant.php` | loads the files below (namespace `SmartTasks\SmartSavant`) |
| `src/Py.php` | `Obj` (an ordered JSON object), `BigInt`, `Json` (reader, and Python's `json.dumps` formatting), `Py` (code-point lengths, `str()`, `repr()`, truthiness, sorting) |
| `src/Gates.php` | `Canonical` (canonical JSON), `Gates` |
| `src/Schema.php` | `Schema` (`plain`, `checkView`, `checkMerge`, `normalizeView`, `checkCouncil`), `NotPlainData` |
| `src/Engine.php` | `Model`, `LLMAdapter`, `StubAdapter`, `ScriptedAdapter` (`AdapterError`), `ReplayAdapter` (`ReplayError`), `Council` (`run`, `health`, `checkPhases`, `loadPhases`, `errorText`) |
| `src/Exporters.php` | `Exporters` (`runDigest`, `toStories`, `toSpecMd`, `toTrainset`, `toMermaid`, `md`, `label`) |
| `src/Audit.php` | `Importer` (`fromSmartsignal`, `detect`, `ideaText`), `Audit` (`auditResult`, `auditStories`, `auditTrainset`, `readJsonl`, `auditPath`) |
| `bin/conformance.php` | the vector runner |
| `composer.json` | package metadata (not published) |

## How to run

From the repository root:

```sh
php ports/php/bin/conformance.php spec/conformance   # last line: php: 198/198 vectors passed
ports/conformance/run.sh php                         # the same, through the shared harness
```

## Data

- A JSON object is an `Obj`: an ordered map with string keys. A PHP array would turn the key `"1"` into
  the integer 1 and would not tell `{}` from `[]`. Use `get($k, $default)`, `has`, `set`, `keys` and
  `foreach`; `$o->name` reads a key.
- A JSON array is a PHP list. Integers stay `int`, and numbers with a fraction or an exponent are `float`,
  as in Python. An integer outside the signed 64-bit range becomes a `BigInt`, which keeps its exact
  digits, so it is written back and hashed as Python writes it.
- Strings are UTF-8. An unpaired surrogate (`"\ud800"` in JSON) is kept as its 3-byte form `ED A0 80`, so
  the checks can report it as Python does. Lengths and cuts count code points.
- `Json::decode()` reads JSON text. `Json::py($v, $sortKeys, $ensureAscii)` writes it as Python's
  `json.dumps` does, so the result of `Council::run()` can be written as a council.json.

## Using it

```php
require 'ports/php/src/SmartSavant.php';

use SmartTasks\SmartSavant\{Audit, Council, Exporters, Json, StubAdapter};

$council = new Council(
    'Learn reusable automations from recorded operator sessions',
    new StubAdapter(),
    ['Developer', 'UX/UI', 'Product Owner'],                     // roster; null: the four default names
    [['frame', 'Frame', 'What is the problem?'], ['build', 'Build', 'How?']],   // phases; null: the defaults
    ['survey-2026'],                                             // source ids views may cite
    2,                                                           // max_loops, 1 to 10
    1,                                                           // retries, 0 to 5
);
$res = $council->run();                    // an Obj: the council.json document

Council::health($res);                     // counts; decided is true only when every phase has a full synthesis
Exporters::toSpecMd($res);                 // Markdown; undecided phases are marked NOT DECIDED
Exporters::toStories($res, 'PROJ');        // ready or blocked stories
Exporters::toTrainset($res);               // methodology rows only for a synthetic run such as this one
Audit::auditResult($res);                  // verdict "not build-ready": a stub run is synthetic
file_put_contents('council.json', Json::py($res) . "\n");
```

Phases may also be `Obj` values with `step_id`, `title` and `question`. `Council::loadPhases($file)` reads a
phases file in the format of `spec/phases.json`. The constructor throws `\InvalidArgumentException` for an
idea, roster name, source or phase string with an unpaired surrogate, for duplicate names or step ids, and
for out-of-range `max_loops` or `retries`.

## Writing an adapter

Extend `LLMAdapter`, set `$name` (and `$synthetic = true` only for stubs and demos), and implement two
methods:

```php
public function view(string $stakeholder, string $role, string $phaseId, string $question, Obj $context): mixed;
public function merge(string $phaseId, string $question, array $views, Obj $context): mixed;
```

`$context` holds `idea`, `iteration`, `sources`, `prior` and `locked`. `$views` is the list of the phase's
valid view records. Both are fresh copies for each call. The answers are described in
`bindings/council.d.ts`, `spec/schema/view.schema.json` and `spec/schema/merge.schema.json`. The prompt
contracts are in `spec/prompts/`. The idea, the sources and the earlier decisions are untrusted text, so
pass them to the model as data, not as instructions.

The engine calls the adapter one call at a time, in roster order. For each answer it:

1. copies the answer to plain data (`Schema::plain`). An answer may be an `Obj`, a `\stdClass` or a PHP
   array; arrays that are not lists become objects, and their integer keys become strings. Other objects,
   resources, strings that are not UTF-8, nesting deeper than 32 and more than 1,000,000 code points of
   text are refused as `"invalid: <message>"`, for example `"invalid: DateTime is not JSON data"`;
2. validates the copy with `checkView` or `checkMerge` and uses only the copy;
3. retries a call that throws or returns an invalid answer, `1 + retries` attempts in all;
4. records a call that never succeeds as `failed` with the error of the last attempt.

An exception is recorded as `"<ShortClassName>: <message>"`, for example `"RuntimeException: HTTP 503"`.
Any `\Throwable` is caught. Unpaired surrogates in the message become `?`, `key=VALUE` secrets and bearer
tokens become `***`, and the text is cut to 500 code points.

The example below posts to a model endpoint of your own. **The URL is an example placeholder**: replace it
with your endpoint, and adapt the request and the response handling to your API.

```php
use SmartTasks\SmartSavant\{Json, LLMAdapter, Obj};

final class MyEndpointAdapter extends LLMAdapter
{
    public string $name = 'my-endpoint';
    private const ENDPOINT = 'https://llm.example.invalid/v1/council';   // EXAMPLE ONLY

    public function __construct(private string $token) {}

    private function call(string $task, array $payload): mixed
    {
        $ctx = stream_context_create(['http' => [
            'method' => 'POST', 'timeout' => 60, 'ignore_errors' => true,
            'header' => "content-type: application/json\r\nauthorization: Bearer {$this->token}\r\n",
            'content' => Json::py(Obj::of(['task' => $task, ...$payload])),
        ]]);
        $body = file_get_contents(self::ENDPOINT, false, $ctx);
        if ($body === false) throw new \RuntimeException('no answer');   // recorded, then retried
        return Json::decode($body);                                       // must be the answer object itself
    }

    public function view(string $stakeholder, string $role, string $phaseId, string $question, Obj $context): mixed
    {
        return $this->call('view', ['stakeholder' => $stakeholder, 'role' => $role, 'phase_id' => $phaseId,
            'question' => $question, 'context' => $context]);
    }

    public function merge(string $phaseId, string $question, array $views, Obj $context): mixed
    {
        return $this->call('merge', ['phase_id' => $phaseId, 'question' => $question, 'views' => $views,
            'context' => $context]);
    }
}
```

## What was tested

PHP 8.3.6 (PCRE 10.42) on Linux x86-64, with the Python reference on Python 3.13.16:

- the 198 vectors;
- a differential run against the Python reference, about 22,000 random cases: strings through every gate,
  `md` and `label`; random views and merges through `checkView`, `checkMerge`, `flagUnsourced` and
  `synthesisOk`; random SmartSignal exports through the importer, the audit and the exporters; random
  trainset rows and stories files through the audit; and random councils (scripted failures, raises,
  retries, loops, flags, non-ASCII text) through every exporter and the audit. Every case where Python
  returned a value gave the same value here;
- about 106,000 further random vectors in the `spec/conformance` file formats (six seeds), generated from
  the Python reference, through `bin/conformance.php`: all passed;
- adapters returning PHP arrays, `\stdClass` and refused types, exception text, and replay of a stub run
  to the same digest.

## Limits

- Where Python raises `TypeError` on an ill-typed input (for example a gate given a number inside a list
  of strings), this port returns a value. Validated runs never reach those paths.
- `Py::repr` decides whether a non-ASCII character is printable from PCRE's Unicode tables, which can
  differ from Python's for recently assigned characters. It is used only for `str()` of non-string values.
- Lower-casing for `complianceBlock` and the trainset's phase-title check uses `mb_strtolower`, which can
  differ from Python's `str.lower()` for a few characters. Only ASCII keywords are searched for.
- The JSON reader refuses nesting deeper than 5,000 levels. Python's limit comes from its recursion
  limit and is lower, so a file nested between those depths is read here and refused by Python.
- A malformed script entry (an object or a number instead of a list) raises this port's `KeyError` or
  `TypeError`, named as Python names them.
- `Audit::auditPath` reads `.json` and `.jsonl` files only. Zip trainsets are read by the Python reference.
  The command line, the demos, `load_adapter`, `write_all` and `write_new` are not ported.
