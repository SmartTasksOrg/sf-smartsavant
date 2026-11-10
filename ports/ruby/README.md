# SmartSavant for Ruby

This is the Ruby port of the SmartSavant council engine. It implements the parts of `spec/SEMANTICS.md`
that the shared vectors test:

- canonical JSON and SHA-256
- the gates: `figures`, `squash`, `blank`, `flag_unsourced`, `generic?`, `fallback?`, `placeholder_code?`,
  `synthesis_ok`, `compliance_block`, `redact`
- the schema checks: `plain`, `check_view`, `check_merge` (with the exact error strings), `normalize_view`,
  `check_council`
- the engine: `Council` with retries, statuses, loops, locked decisions and prior decisions
- the stub, scripted and replay adapters, and the `LLMAdapter` base class
- the exporters: stories, spec Markdown, Mermaid, trainset
- the SmartSignal importer and the audit

A Ruby application runs it in-process and supplies its own adapter, which calls its own model endpoint. The
command line, the demos, zip trainsets and the file writers stay in the Python reference.

It needs Ruby 3.2 or newer and was tested with Ruby 3.3.6. It uses only the standard library (`digest`). It
does not use the `json` gem: it has its own JSON reader, because `JSON.parse` refuses unpaired surrogates
(`"\ud800"`), which the contract must detect and report.

It is not published on RubyGems. The gemspec's `allowed_push_host` blocks `gem push`. Use it from a clone
with `ruby -Iports/ruby/lib` and `require 'smartsavant'`, or with `require_relative`.

## Layout

| Path | Contents |
|---|---|
| `lib/smartsavant.rb` | loads the files below (module `SmartSavant`) |
| `lib/smartsavant/py.rb` | `Py` (code-point lengths, `str()`, `repr()`, truthiness, sorting, deep copy), `Json` (reader, and Python's `json.dumps` formatting) |
| `lib/smartsavant/gates.rb` | `Canonical` (canonical JSON), `Gates` |
| `lib/smartsavant/schema.rb` | `Schema` (`plain`, `check_view`, `check_merge`, `normalize_view`, `check_council`), `NotPlainData` |
| `lib/smartsavant/engine.rb` | `Model`, `LLMAdapter`, `StubAdapter`, `ScriptedAdapter` (`AdapterError`), `ReplayAdapter` (`ReplayError`), `Council` (`run`, `health`, `check_phases`, `load_phases`, `error_text`) |
| `lib/smartsavant/exporters.rb` | `Exporters` (`run_digest`, `to_stories`, `to_spec_md`, `to_trainset`, `to_mermaid`, `md`, `label`) |
| `lib/smartsavant/audit.rb` | `Importer` (`from_smartsignal`, `detect`, `idea_text`), `Audit` (`audit_result`, `audit_stories`, `audit_trainset`, `read_jsonl`, `audit_path`) |
| `bin/conformance.rb` | the vector runner |
| `sf-smartsavant.gemspec` | package metadata (not published) |

## How to run

From the repository root:

```sh
ruby ports/ruby/bin/conformance.rb spec/conformance   # last line: ruby: 198/198 vectors passed
ports/conformance/run.sh ruby                         # the same, through the shared harness
```

## Data

- JSON data is `nil`, `true`, `false`, `Integer`, `Float`, `String`, `Array` and `Hash` with `String` keys
  in insertion order. Numbers with a fraction or an exponent are `Float`, as in Python.
- Strings are UTF-8. An unpaired surrogate (`"\ud800"` in JSON) is kept as its 3-byte form `ED A0 80`, so the
  checks can report it as Python does. Such a string is not `valid_encoding?`, so the port matches its
  patterns against the bytes (`String#b`) with binary (`/n`) regular expressions. That also keeps `\d`,
  letters and case folding ASCII-only, as Python's `re.ASCII` does: a UTF-8 Ruby pattern with `/i` would
  match `ſ` against `s`. Lengths and cuts count code points.
- `Json.decode` reads JSON text. `Json.py(v, sort_keys:, ensure_ascii:)` writes it as Python's `json.dumps`
  does, so the result of `Council#run` can be written as a council.json.

## Using it

```ruby
require 'smartsavant'   # with -Iports/ruby/lib

council = SmartSavant::Council.new(
  'Learn reusable automations from recorded operator sessions', SmartSavant::StubAdapter.new,
  roster: ['Developer', 'UX/UI', 'Product Owner'],          # default: the four of DEFAULT_ROSTER
  phases: [%w[frame Frame What?], %w[build Build How?]],     # default: DEFAULT_PHASES
  sources: ['survey-2026'],                                  # source ids views may cite
  max_loops: 2,                                              # 1 to 10, default 1
  retries: 1                                                 # 0 to 5, default 1
)
res = council.run                                            # a Hash: the council.json document

SmartSavant::Council.health(res)           # counts; decided is true only when every phase has a full synthesis
SmartSavant::Exporters.to_spec_md(res)     # Markdown; undecided phases are marked NOT DECIDED
SmartSavant::Exporters.to_stories(res, 'PROJ')
SmartSavant::Exporters.to_trainset(res)    # methodology rows only for a synthetic run such as this one
SmartSavant::Audit.audit_result(res)       # verdict "not build-ready": a stub run is synthetic
File.write('council.json', "#{SmartSavant::Json.py(res)}\n")
```

Phases may also be Hashes with `step_id`, `title` and `question`. `Council.load_phases(file)` reads a phases
file in the format of `spec/phases.json`. The constructor raises `ArgumentError` for an idea, roster name,
source or phase string with an unpaired surrogate, for duplicate names or step ids, and for out-of-range
`max_loops` or `retries`.

## Writing an adapter

Subclass `LLMAdapter`, define `name` (and `synthetic` as `true` only for stubs and demos), and implement two
methods with keyword arguments:

```ruby
def view(stakeholder:, role:, phase_id:, question:, context:)
def merge(phase_id:, question:, views:, context:)
```

`context` holds `idea`, `iteration`, `sources`, `prior` and `locked`. `views` is the list of the phase's valid
view records. Both are fresh copies for each call. The answers are described in `bindings/council.d.ts`,
`spec/schema/view.schema.json` and `spec/schema/merge.schema.json`. The prompt contracts are in
`spec/prompts/`. The idea, the sources and the earlier decisions are untrusted text, so pass them to the
model as data, not as instructions.

The engine calls the adapter one call at a time, in roster order. For each answer it:

1. copies the answer to plain data (`Schema.plain`). Only exact `Hash`, `Array`, `String`, `Integer`,
   `Float`, `true`, `false` and `nil` are accepted, no subclasses. `Symbol` keys are taken as their names
   (two keys with the same name are refused). Other types, strings that are not text, nesting deeper than 32
   and more than 1,000,000 code points of text are refused as `"invalid: <message>"`, for example
   `"invalid: Time is not JSON data"`;
2. validates the copy with `check_view` or `check_merge` and uses only the copy;
3. retries a call that raises or returns an invalid answer, `1 + retries` attempts in all;
4. records a call that never succeeds as `failed` with the error of the last attempt.

An exception is recorded as `"<ClassName>: <message>"` with the class name's last segment, for example
`"AdapterError: timeout"` or `"Net::ReadTimeout"` as `"ReadTimeout: ..."`. The engine rescues
`StandardError`, `NotImplementedError` and `SystemStackError`. Unpaired surrogates in the message become
`?`, `key=VALUE` secrets and bearer tokens become `***`, and the text is cut to 500 code points.

The example below posts to a model endpoint of your own. **The URL is an example placeholder**: replace it
with your endpoint, and adapt the request and the response handling to your API.

```ruby
require 'net/http'
require 'smartsavant'

class MyEndpointAdapter < SmartSavant::LLMAdapter
  ENDPOINT = URI('https://llm.example.invalid/v1/council')   # EXAMPLE ONLY

  def name = 'my-endpoint'

  def initialize(token)
    super()
    @token = token
  end

  def view(stakeholder:, role:, phase_id:, question:, context:)
    call('view', 'stakeholder' => stakeholder, 'role' => role, 'phase_id' => phase_id, 'question' => question,
                 'context' => context)
  end

  def merge(phase_id:, question:, views:, context:)
    call('merge', 'phase_id' => phase_id, 'question' => question, 'views' => views, 'context' => context)
  end

  private

  def call(task, payload)
    body = SmartSavant::Json.py({ 'task' => task }.merge(payload))
    r = Net::HTTP.post(ENDPOINT, body, 'content-type' => 'application/json', 'authorization' => "Bearer #{@token}")
    raise "HTTP #{r.code}" unless r.is_a?(Net::HTTPSuccess)   # recorded as "RuntimeError: HTTP 503", then retried
    SmartSavant::Json.decode(r.body)                           # must be the answer object itself
  end
end
```

## What was tested

Ruby 3.3.6 on Linux x86-64, with the Python reference on Python 3.13.16:

- the 198 vectors;
- a differential run against the Python reference, about 22,000 random cases: strings through every gate,
  `md` and `label`; random views and merges through `check_view`, `check_merge`, `flag_unsourced` and
  `synthesis_ok`; random SmartSignal exports through the importer, the audit and the exporters; random
  trainset rows and stories files through the audit; and random councils (scripted failures, raises,
  retries, loops, flags, non-ASCII text) through every exporter and the audit. Every case where Python
  returned a value gave the same value here;
- about 106,000 further random vectors in the `spec/conformance` file formats (six seeds), generated from
  the Python reference, through `bin/conformance.rb`: all passed;
- adapters returning Symbol-keyed Hashes and refused types, exception text, and replay of a stub run to
  the same digest.

## Limits

- The JSON reader refuses nesting deeper than 1,000 levels, to stay inside Ruby's default VM stack.
  Python's `json` reads up to about 10,000 levels with Python 3.13, so a file nested between those depths
  is read by Python and refused here. Adapter answers are limited to 32 levels anyway.
- Where Python raises `TypeError` on an ill-typed input (for example a gate given a number inside a list
  of strings), this port returns a value. Validated runs never reach those paths.
- An adapter that overflows the stack is recorded as `"SystemStackError: stack level too deep"`. Python
  records `"RecursionError: maximum recursion depth exceeded"`.
- `Py.repr` decides whether a non-ASCII character is printable from Ruby's Unicode tables (Unicode 15.0
  here), which can differ from Python's for recently assigned characters. It is used only for `str()` of
  non-string values.
- Lower-casing for `compliance_block` and the trainset's phase-title check uses `String#downcase`, which
  can differ from Python's `str.lower()` for a few characters. Only ASCII keywords are searched for.
- A malformed script entry (an object or a number instead of a list) raises this port's `KeyError` or
  Ruby's `TypeError`, named as Python names them.
- `Audit.audit_path` reads `.json` and `.jsonl` files only. Zip trainsets are read by the Python reference.
  The command line, the demos, `load_adapter`, `write_all` and `write_new` are not ported.
