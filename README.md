# SmartSavant

SmartSavant runs a **design council** around a language model and checks what comes out before anyone
builds or trains from it.

A roster of stakeholders (Developer, UX/UI, Product Owner, Architecture/Infra, or your own) gives a
structured view on each phase of a design sprint: frame, concept, design, build, integrate, measure and
spec. Each view covers who, what, where, when, why and how, what is done, what is missing, a
devil's-advocate risk and a contribution. A juror merges the views into a decision. The run is exported
as a spec, user stories, a decision tree and a fine-tune trainset.

What it adds over a prompt chain:

- **Model-agnostic.** The engine talks only to an adapter with two calls, `view` and `merge`. The
  package ships a deterministic stub, a scripted adapter and a replay adapter; your adapter calls your
  model.
- **Nothing invented on failure.** Every adapter answer is checked against the contract and retried
  when invalid. A view or merge that never becomes valid is recorded as `failed`, with the error,
  instead of being filled with template text. A merge that is valid but decides nothing usable is
  recorded as `thin`.
- **Exports that say what is not decided.**
  - A phase without a full synthesis becomes a *blocked* story and a **NOT DECIDED** section in the
    spec, never a generic "Advance the build with the decided approach" story.
  - The trainset never learns from synthetic runs, failed merges or flagged views.
- **Gates.**
  - Figures stated without a source are flagged, and so are sources the run did not provide.
  - Every adapter answer is copied to plain JSON data before it is checked. Model text is escaped in
    Markdown and on the terminal.
  - Fallback text, placeholder code and generic next steps are detected.
  - A keyword compliance stub has a place for your own policy.
- **Audits of any council run**: its own, or an export of the SmartSignal council. `sf-smartsavant
  audit` answers one question, "is there a decision here to build from?", and exits non-zero when there
  is not.

The engine, gates, exporters, importer and audit are specified in [spec/SEMANTICS.md](spec/SEMANTICS.md).
They are implemented in Python and in seven ports, and all eight engines pass the same 198 vectors:

| Port | Folder | Toolchain it was tested with | Package name (not published) |
|---|---|---|---|
| Node.js | `ports/node` | Node 22 | npm `sf-smartsavant` (private) |
| Go | `ports/go` | Go 1.24 (module declares 1.22) | module `github.com/SmartTasksOrg/sf-smartsavant/ports/go` |
| Rust | `ports/rust` | Rust 1.97, no crates | crate `sf-smartsavant` (`publish = false`) |
| Java | `ports/java` | JDK 21, compiled with `--release 17` | Maven `cloud.smarttasks:sf-smartsavant` |
| C# | `ports/csharp` | .NET SDK 8.0, no NuGet packages | assembly `SfSmartSavant` (not packable) |
| PHP | `ports/php` | PHP 8.3 (needs 8.1+, `mbstring`) | Composer `smarttasksorg/sf-smartsavant` |
| Ruby | `ports/ruby` | Ruby 3.3 (needs 3.2+) | gem `sf-smartsavant` |

Every port uses only its language's standard library. `ports/conformance/run.sh` runs the vectors on
every engine whose toolchain is installed. Each port's README lists the documented differences from
Python (for example how the language stores unpaired surrogates) and what was tested.

## Install

```bash
python -m pip install sf-smartsavant
sf-smartsavant demo
```

Every file of `sf-smartsavant` on PyPI is built and published by this repository's release workflow
(`.github/workflows/release.yml`, PyPI trusted publishing). Each file carries a provenance attestation
that names this repository and that workflow; PyPI shows it under "Verified details". The same
workflow records a GitHub attestation, which you can check with
`gh attestation verify <file> --repo SmartTasksOrg/sf-smartsavant`. A release file without that
provenance is not ours, and neither is a package called `smartsavant` (without `sf-`) on any registry.

Version 0.1.0 is the first release under this name. If pip cannot find it yet, that release has not
finished. Install from a clone instead (Python 3.10 or later, no dependencies):

```bash
git clone https://github.com/SmartTasksOrg/sf-smartsavant
cd sf-smartsavant
python -m venv .venv
. .venv/bin/activate          # Windows PowerShell: .\.venv\Scripts\Activate.ps1
python -m pip install .
sf-smartsavant demo
```

The demo runs three councils in memory, with no model and no network:
1. a stub council, fully decided, which the audit still refuses because a stub is not a deliberation;
2. a council whose adapter breaks, so that failures are recorded, stories blocked and the audit says
   "not build-ready";
3. a replay of the broken run from its own record.

## Use it

```bash
# a run with the stub (deterministic, synthetic): shows the files you get
sf-smartsavant run --idea "Learn reusable automations from recorded operator sessions" --out run1
sf-smartsavant audit run1/council.json run1/stories.json run1/trainset.jsonl

# a run with your own adapter (module:Class on your Python path; settings as KEY=VALUE)
sf-smartsavant run --idea-file idea.txt --adapter mycompany.savant:ModelAdapter \
    --adapter-arg model=my-model --roster "Developer,Security,Operator" --sources survey-2026,logs-q3 --out run2

# audit an export of the SmartSignal council before building from it
sf-smartsavant audit council_stakeholder_rounds.json SmartSavant_user_stories.json SmartSavant_gv_trainset.zip
sf-smartsavant import council_result.json --out council.json   # then export / audit like any run
```

`audit` exits 0 when every file is build-ready, 1 when one is not, and 2 when one cannot be read.
`run` and `export` write `council.json`, `spec.md`, `stories.json`, `trainset.jsonl` and
`decision_tree.mmd`. They never overwrite existing files.

## In Python

```python
from sf_smartsavant import Council, LLMAdapter, audit_result, health
from sf_smartsavant.exporters import to_spec_md, to_stories

class ModelAdapter(LLMAdapter):
    name = "my-model"
    def view(self, *, stakeholder, role, phase_id, question, context):
        ...   # call your model; return {"w5h1": {...}, "done_so_far", "missing", "devils_advocate", "contribution", "sources"}
    def merge(self, *, phase_id, question, views, context):
        ...   # return {"merge_summary", "decision", "resolved", "open_issues", "next_steps", "spec_delta", "loop_recommended"}

res = Council(idea="...", adapter=ModelAdapter(), max_loops=2, retries=1).run()
print(health(res), audit_result(res)["verdict"])
```

[docs/adapters.md](docs/adapters.md) explains how to write an adapter:
- the contract for each call;
- schema-constrained decoding;
- treating the idea and the sources as untrusted data;
- timeouts and retries.

The JSON schemas are in `spec/schema/`, and the prompt contracts in `spec/prompts/`.

## What the audit found in a real export

The SmartSavant council run delivered with this project (October 2026) is the reason the audit exists.
- All 84 stakeholder views were offline templates.
- All 14 juror merges failed with "unparseable output" and were replaced with fallback text.
- All 9 user stories were "Advance the X with the decided approach".
- 7 of the 31 trainset rows taught fallback or placeholder output.

`sf-smartsavant audit` reports all of this, with the verdict "not build-ready". An earlier, working
council run of a sibling project passes the same audit (with warnings for unsourced figures).
[docs/audit.md](docs/audit.md) lists every finding.

## Status

- **Version 0.1.0, experimental.** It ships the engine, the stub, scripted and replay adapters, the
  gates, the exporters, the SmartSignal importer, the audit, a command line and seven ports (Node.js,
  Go, Rust, Java, C#, PHP, Ruby).
- **Published:** PyPI `sf-smartsavant` (see Install). Nothing else is published; the ports are used from a
  clone.
- **Tested:**
  - The Python tests and the install-line check gate every pull request (`.github/workflows/ci.yml`).
  - The `ports` job runs the 198 vectors on Python and Node and gates merges.
  - The `ports-all` job runs the vectors and each port's own tests on Go, Rust, Java, C#, PHP and Ruby.
    It has not run on GitHub yet, so it is shown but does not gate merges. All eight engines passed
    198/198 on the maintainers' Linux machine on 2026-10-16.
- **Not included:**
  - **A real model adapter.** Adapters are yours. The engine does not call a model, does not judge
    whether a decision is good, and does not make a weak model deliberate well.
  - **The gates are heuristics.** The compliance gate is a keyword stub to replace with your policy.
  - **An HTTP service and a user interface.** It is a library and a command line.
- **Origin:**
  - This package extends `smartsavant-core` 0.1.0 (the maintainers' reference core); see
    [docs/decisions.md](docs/decisions.md) for what changed and why.
  - The SmartSavant council run did not produce a usable specification (see above), so this design is
    the maintainers', not the council's.
- **Security review:** one pre-release review of the code by a separate AI reviewer that did not write
  it. Its findings, and those of the maintainers' private robustness suite, were fixed (see CHANGELOG).
  There has been no independent human audit. Report vulnerabilities as described in
  [SECURITY.md](SECURITY.md).

## The Smart* family

SmartSavant is part of the Smart* family by SmartTasks Lab:
- [SmartGate](https://github.com/SmartTasksOrg/sf-smartgate): decision friction and a human override for single actions;
- [SmartSwitch](https://github.com/SmartTasksOrg/sf-smartswitch): a quorum kill switch for agent fleets;
- [SmartWatch](https://github.com/SmartTasksOrg/sf-smartwatch): a coupling-density monitor for multi-agent systems;
- [SmartDelegate](https://github.com/SmartTasksOrg/sf-smartdelegate): IAM for LLM agents;
- [SmartRoute](https://github.com/SmartTasksOrg/sf-smartroute);
- [SmartSeal](https://github.com/SmartTasksOrg/sf-smartseal);
- [SmartCheck](https://github.com/SmartTasksOrg/sf-smartcheck);
- [SmartFabric](https://github.com/SmartTasksOrg/sf-smartfabric);
- the others listed on [smarttasks.cloud](https://smarttasks.cloud).

Every family package is on PyPI under its `sf-` name; a package without the `sf-` prefix is not ours.

## Contributing, security, licence

[CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md) · [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) ·
Apache-2.0 ([LICENSE](LICENSE), [NOTICE](NOTICE)).
