# Prompt contract — juror merge (adapter.merge)

The juror receives all stakeholder views for a phase and MUST return JSON matching
`#/$defs/juror`. This is the facilitation step the quality gate guards: a merge that
does not `decision` + `merge_summary` + at least one `resolved` item is treated as a
salvaged fallback and excluded from the fine-tune trainset.

Required output keys: `merge_summary`, `resolved[]`, `open_issues[]`, `next_steps[]`,
`spec_delta`, `decision`, `loop_recommended`.

Set `loop_recommended: true` only when open issues genuinely block the next phase.
