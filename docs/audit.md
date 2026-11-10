# Auditing a council run

```bash
sf-smartsavant audit FILE [FILE ...] [--json]
```

A file can be one of:
- a `council.json` written by this package;
- a SmartSignal council export (`council_result.json`, `council_stakeholder_rounds*.json`, a pack's `rounds.json` /
  `rounds_nested.json`);
- a stories file (`*_user_stories.json`, or `stories.json` from this package);
- a trainset (`.jsonl`, or a `.zip` whose `*.jsonl` members are read; `flagged*` members are skipped).

Exit codes: 0 when every file is build-ready, 1 when one is not, 2 when one cannot be read.

## Findings

| Code | Severity | Meaning |
|---|---|---|
| AUDIT-NO-SYNTHESIS | error | A phase's last merge is not a full synthesis (or stories are blocked). |
| AUDIT-OFFLINE-VIEWS | error (warning when fewer than half) | Views came from an offline template, not a model. |
| AUDIT-FALLBACK-TEXT | error | Fallback text ("Juror synthesis failed ...") stands where a decision should be. |
| AUDIT-TEMPLATE-VIEWS | error | The same view text appears in three or more phases. |
| AUDIT-GENERIC-STEPS | warning (error when all) | Next steps or story titles name nothing. |
| AUDIT-COMPLIANCE | error | A compliance rule fired. |
| AUDIT-SYNTHETIC | error | A stub or demo run: fine for tests, never a basis for building. |
| AUDIT-TRAINSET-CONTAMINATED | error | Trainset rows teach fallback or placeholder output. |
| AUDIT-TRAINSET-UNREADABLE | error | Trainset lines that are not JSON objects with an `output` or `messages`. |
| AUDIT-FAILED-VIEWS | warning | Views the adapter never delivered validly. |
| AUDIT-PLACEHOLDER-CODE | warning | `# TODO` or `return {'ok': True}` presented as a contribution. |
| AUDIT-UNSOURCED | warning | Figures without a source, or sources the run did not provide. |

A `council.json` is not taken at its word:
- a merge recorded as `full` that fails the synthesis gate counts as not decided;
- the adapters `stub`, `scripted` and `demo*` count as synthetic whatever the file says.

The verdict is `build-ready` only without errors. Warnings deserve a look: unsourced figures in
particular, because a council will happily invent "40% activation in week 1".

## What it found in the two exports delivered with this project

| Export | Verdict | Main findings |
|---|---|---|
| SmartSavant council, October 2026 (rounds, stories, trainset) | not build-ready | 84/84 views offline; 14/14 merges failed (fallback text); 14/14 next steps and 9/9 stories generic; template views; 7/31 trainset rows contaminated |
| SmartWatch council, October 2026 (result, rounds, stories, trainset) | build-ready | 7/7 phases decided, 42 model views; 86 figures without a source (warning) |

The audit cannot tell whether a decision is right. A council can decide confidently and wrongly. The
audit tells whether there is a decision at all, and whether what looks like one is really fallback
text.
