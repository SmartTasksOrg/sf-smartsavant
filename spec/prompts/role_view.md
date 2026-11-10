# Prompt contract — stakeholder view (adapter.view)

A private adapter sends the model a prompt built from these fields and MUST return
JSON matching `#/$defs/view` in council.schema.json. Grammar-constrain it (GBNF /
JSON-schema decoding) so the shape is guaranteed — do not post-hoc regex-salvage.

Inputs available: `stakeholder`, `role`, `phase_id`, `question`, `context.idea`,
`context.sources`, `context.prior` (earlier decisions).

Required output keys: `w5h1{who,what,where,when,why,how}`, `done_so_far`,
`missing`, `devils_advocate`, `contribution`, `sources[]`.

Rules:
- Every figure in `contribution`/`devils_advocate` MUST cite a `sources[]` id, or it
  will be flagged `unsourced_figures` by the gate.
- `devils_advocate` is required and must name a concrete risk, not a platitude.
