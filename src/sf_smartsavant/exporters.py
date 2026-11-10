"""Exporters: a council result -> user stories, a spec in Markdown, a fine-tune trainset, a Mermaid tree.

Every exporter reports what the run did NOT decide instead of covering it up:
- a phase whose last merge is not a full synthesis becomes a ``blocked`` story ("... was not decided"),
  never a story titled with a generic step;
- the spec opens with the run's health and marks undecided phases;
- the trainset learns only from full syntheses of non-synthetic runs, and only from views without flags.
Formats: spec/SEMANTICS.md, "Exports"; vectors: spec/conformance/runs.json.
"""
from __future__ import annotations

import errno
import json
from dataclasses import asdict

from . import canonical, gates
from .engine import health
from .model import CouncilResult


def run_digest(res: CouncilResult) -> str:
    """sha256 of the canonical JSON of the whole result: the run's identity in exports."""
    import hashlib
    return hashlib.sha256(canonical.encode(res.to_dict())).hexdigest()


def _last_rounds(res: CouncilResult) -> list:
    """The last iteration of each phase, in phase order."""
    last: dict = {}
    for r in res.rounds:
        last[r.step_id] = r
    return list(last.values())


def to_stories(res: CouncilResult, project_key: str) -> dict:
    stories = []
    for i, r in enumerate(_last_rounds(res), 1):
        j = r.juror
        sid = f'STORY-{project_key}-{i:04d}'
        steps = [s for s in j.next_steps if not gates.is_generic(s)]
        if j.synthesis_status == 'full' and steps:
            stories.append({
                'story_id': sid, 'status': 'ready', 'epic': r.step_title, 'title': steps[0],
                'decision': j.decision, 'sub_tasks': steps,
                'acceptance_criteria': [f'Implements the decision: {j.decision}',
                                        *[f'Holds: {x}' for x in j.resolved],
                                        'A test exercises it end to end.'],
                'open_issues': list(j.open_issues), 'phase': r.step_id, 'iteration': r.iteration,
            })
        else:
            stories.append({
                'story_id': sid, 'status': 'blocked', 'epic': r.step_title,
                'title': f'Decide {r.step_title}: the council did not reach a decision ({j.synthesis_status})',
                'decision': '', 'sub_tasks': [], 'acceptance_criteria': [],
                'open_issues': list(j.open_issues) + ([j.error] if j.error else []),
                'phase': r.step_id, 'iteration': r.iteration,
            })
    return {'schema': 'smartsavant.stories/1', 'project_key': project_key, 'run': run_digest(res),
            'counts': {'stories': len(stories), 'ready': sum(s['status'] == 'ready' for s in stories),
                       'blocked': sum(s['status'] == 'blocked' for s in stories)},
            'stories': stories}


_MD_META = set('\\`*_{}[]()#+-.!|~')
# control characters (C0 except the whitespace squash removes, DEL, C1) and bidirectional controls: never
# written into a document (a terminal running `cat spec.md` would execute escape sequences)
_BAD = set(chr(c) for c in list(range(32)) + list(range(127, 160)) + list(range(0x202A, 0x202F)) + list(range(0x2066, 0x206A)))


def _md(text: str) -> str:
    """Model text made inert in Markdown: one line (whitespace squashed), ``&``, ``<`` and ``>`` as HTML
    entities, control and bidirectional-control characters as U+FFFD, and every Markdown metacharacter ``\\ ` * _ { } [ ] ( ) # + - . ! | ~`` backslash-escaped,
    so it can start no heading, list, fence, link, image, emphasis or table cell."""
    out = []
    for c in gates.squash(str(text)):
        if c in _BAD:
            out.append('\ufffd')
        elif c == '&':
            out.append('&amp;')
        elif c == '<':
            out.append('&lt;')
        elif c == '>':
            out.append('&gt;')
        elif c in _MD_META:
            out.append('\\' + c)
        else:
            out.append(c)
    return ''.join(out)


def to_spec_md(res: CouncilResult) -> str:
    h = health(res)
    lines = ['# Council spec', '', f'*{_md(res.idea)}*', '',
             f'Run {run_digest(res)[:16]} with adapter {_md(res.adapter)}'
             f'{" (synthetic: not a real deliberation)" if res.synthetic else ""}. '
             f'{h["phases_decided"]} of {h["phases"]} phases decided; views {h["views"]}; merges {h["merges"]}; '
             f'{h["flags"]} flag(s).', '']
    if not h['decided']:
        lines += ['> **Not build-ready:** at least one phase has no full synthesis. Do not build from the '
                  'undecided phases; re-run them with a working adapter.', '']
    if res.locked_decisions:
        lines += ['## Decisions', '', '| # | Decision |', '|---|---|']
        lines += [f'| {i} | {_md(d)} |' for i, d in enumerate(res.locked_decisions, 1)]
        lines += ['']
    for r in _last_rounds(res):
        j = r.juror
        lines += [f'## {_md(r.step_title)}', '', f'**Question:** {_md(r.question)}', '']
        if j.synthesis_status == 'full':
            lines += [f'**Decision:** {_md(j.decision)}', '', _md(j.merge_summary), '']
            if j.next_steps:
                lines += ['**Next steps:**', ''] + [f'- {_md(s)}' for s in j.next_steps] + ['']
        else:
            lines += [f'**NOT DECIDED** ({j.synthesis_status}){": " + _md(j.error) if j.error else ""}', '']
        if j.open_issues:
            lines += ['**Open issues:**', ''] + [f'- {_md(s)}' for s in j.open_issues] + ['']
        failed = [v.stakeholder for v in r.views if v.status != 'ok']
        if failed:
            lines += [f'*No valid view from: {_md(", ".join(failed))}.*', '']
    if res.flags:
        lines += ['## Flags', '', f'{len(res.flags)} finding(s) to review before use:', '']
        for f in res.flags[:200]:
            what = f.get('figure') or f.get('source') or ''
            lines += [f'- {f["flag"]}: {_md(what)} ({_md(f.get("stakeholder", ""))}, {_md(f.get("step_id", ""))})']
        lines += ['']
    return '\n'.join(lines)


METHODOLOGY = {  # guidance per default phase; used only when the round's title starts with the phase name
    'frame': 'Establish the problem, the real users and their jobs-to-be-done, and agree on one long-term goal.',
    'concept': 'Collect solution directions, then pick the wedge: the smallest offer the users would switch for.',
    'design': 'Draw the user flow and the data flow end to end, and name every screen and every store.',
    'build': 'Decide the technical approach and the riskiest part to prove first.',
    'integrate': 'Fit the design into the target architecture: owners, interfaces, auth, deployment.',
    'measure': 'Set the KPIs, how each one is measured, and the thresholds that would change the plan.',
    'spec': 'Assemble the decisions into one document that an engineer can build from.',
}


def to_trainset(res: CouncilResult, allow_synthetic: bool = False) -> list:
    """Rows ``{instruction, input, output, meta}``:
    - ``facilitation``: a juror merge, only when its synthesis is full;
    - ``expert``: a stakeholder view with status ok and no flag;
    - ``methodology``: fixed guidance for the phases this run used (one row per phase).
    A synthetic run (stub, demo) yields methodology rows only, unless ``allow_synthetic`` (tests)."""
    digest = run_digest(res)
    rows = []
    flagged = {(f.get('step_id'), f.get('iteration'), f.get('stakeholder')) for f in res.flags}
    learn = allow_synthetic or not res.synthetic
    seen = set()
    for r in res.rounds:
        meta = {'phase': r.step_id, 'iteration': r.iteration, 'run': digest, 'adapter': res.adapter}
        if r.step_id not in seen and r.step_id in METHODOLOGY and r.step_title.lower().startswith(r.step_id):
            seen.add(r.step_id)
            rows.append({'instruction': f'We are at the {r.step_title} phase of a design council. How should I '
                                        f'facilitate it?', 'input': '', 'output': METHODOLOGY[r.step_id],
                         'meta': {**meta, 'kind': 'methodology'}})
        if not learn:
            continue
        for v in r.views:
            if v.status == 'ok' and (r.step_id, r.iteration, v.stakeholder) not in flagged:
                rows.append({'instruction': f'As the {v.stakeholder} on a design council, give your view on: {r.question}',
                             'input': res.idea,
                             'output': json.dumps({k: asdict(v)[k] for k in ('w5h1', 'done_so_far', 'missing',
                                                   'devils_advocate', 'contribution', 'sources')},
                                                  ensure_ascii=False, sort_keys=True),
                             'meta': {**meta, 'kind': 'expert', 'stakeholder': v.stakeholder}})
        if r.juror.synthesis_status == 'full':
            j = r.juror
            rows.append({'instruction': f'Merge the panel for the {r.step_title} phase as the juror.',
                         'input': json.dumps([v.contribution for v in r.views if v.status == 'ok'], ensure_ascii=False),
                         'output': json.dumps({'decision': j.decision, 'merge_summary': j.merge_summary,
                                               'resolved': j.resolved, 'next_steps': j.next_steps},
                                              ensure_ascii=False, sort_keys=True),
                         'meta': {**meta, 'kind': 'facilitation', 'synthesis': 'full'}})
    return rows


def _label(t: str) -> str:
    """A Mermaid label: squashed, quotes as apostrophes, < > & as entities, at most 80 code points, quoted."""
    t = ''.join('\ufffd' if c in _BAD else c for c in gates.squash(str(t)))[:80]
    return '"' + t.replace('&', '&amp;').replace('"', "'").replace('<', '&lt;').replace('>', '&gt;') + '"'


def to_mermaid(res: CouncilResult) -> str:
    q = _label
    lines = ['flowchart TD', '  start([Idea])']
    prev = 'start'
    for i, n in enumerate(res.decision_tree):
        nid = f'n{i}'
        r = res.rounds[i]
        label = n.decision if r.juror.synthesis_status == 'full' else f'NOT DECIDED: {r.juror.synthesis_status}'
        lines.append(f'  {nid}{{{q(r.step_title + " #" + str(n.iteration))}}}')
        lines.append(f'  {prev} --> {nid}')
        lines.append(f'  {nid}d[{q(label)}]')
        lines.append(f'  {nid} --> {nid}d')
        prev = nid
    return '\n'.join(lines) + '\n'


def write_all(res: CouncilResult, out_dir: str, project_key: str = 'PROJECT') -> dict:
    """council.json, spec.md, stories.json, trainset.jsonl, decision_tree.mmd in ``out_dir`` (created;
    existing files are not overwritten)."""
    import os
    os.makedirs(out_dir, exist_ok=True)
    files = {
        'council.json': json.dumps(res.to_dict(), indent=1, ensure_ascii=False) + '\n',
        'spec.md': to_spec_md(res),
        'stories.json': json.dumps(to_stories(res, project_key), indent=1, ensure_ascii=False) + '\n',
        'trainset.jsonl': ''.join(json.dumps(r, ensure_ascii=False, sort_keys=True) + '\n' for r in to_trainset(res)),
        'decision_tree.mmd': to_mermaid(res),
    }
    for name in files:                                  # all or nothing: refuse before writing anything
        if os.path.lexists(os.path.join(out_dir, name)):
            raise FileExistsError(errno.EEXIST, 'exists', os.path.join(out_dir, name))
    for name, text in files.items():
        write_new(os.path.join(out_dir, name), text)
    return {k: len(v) for k, v in files.items()}


def write_new(path: str, text: str) -> None:
    """Write a new file completely or not at all: a temporary file in the same folder, then a hard link
    to the final name (which fails if the name exists, so nothing is ever overwritten)."""
    import os
    import tempfile
    data = text.encode('utf-8')                         # fails before any file is created
    d = os.path.dirname(os.path.abspath(path))
    fd, tmp = tempfile.mkstemp(dir=d, prefix='.tmp-savant-')
    try:
        with os.fdopen(fd, 'wb') as f:
            f.write(data)
        os.link(tmp, path)
    finally:
        os.unlink(tmp)
