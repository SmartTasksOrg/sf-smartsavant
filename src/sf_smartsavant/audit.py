"""Audit a council run (or its stories, or its trainset) before anyone builds or trains from it.

    sf-smartsavant audit council.json              # a run of this package, or a SmartSignal export
    sf-smartsavant audit stories.json trainset.zip # stories and trainsets are checked too

The verdict is ``build-ready`` only when no finding has severity ``error``. Findings (codes are stable;
spec/SEMANTICS.md, "Audit"):

    AUDIT-NO-SYNTHESIS     error    a phase's last merge is not a full synthesis
    AUDIT-OFFLINE-VIEWS    error    views came from an offline template, not a model (warning when < half)
    AUDIT-FAILED-VIEWS     warning  views the adapter never delivered validly
    AUDIT-FALLBACK-TEXT    error    fallback text ("synthesis failed ...") in summaries, decisions or spec deltas
    AUDIT-TEMPLATE-VIEWS   error    the same view text repeated across phases (a template, not deliberation)
    AUDIT-GENERIC-STEPS    warning  next steps or story titles that name nothing (error when all are generic)
    AUDIT-PLACEHOLDER-CODE warning  "TODO" / ``return {'ok': True}`` code presented as a contribution
    AUDIT-UNSOURCED        warning  figures without a source, or sources the run did not provide
    AUDIT-COMPLIANCE       error    a compliance rule fired (gates.compliance_block)
    AUDIT-SYNTHETIC        error    a synthetic run (stub, demo): fine for tests, never for building
    AUDIT-TRAINSET-CONTAMINATED error  trainset rows whose output is fallback, template or placeholder text
    AUDIT-TRAINSET-UNREADABLE   error  trainset lines that are not JSON objects with an output or messages

A council.json is not taken at its word: a merge recorded as ``full`` that fails gates.synthesis_ok
counts as not decided, and the adapters ``stub``, ``scripted`` and ``demo-*`` count as synthetic
whatever the file says.

An audit cannot tell whether a decision is right. It tells whether there is a decision at all.
"""
from __future__ import annotations

import io
import json
import zipfile

from . import gates
from .engine import health
from .model import CouncilResult

MAX_EXAMPLES = 5


def _f(code, severity, count, detail, examples=()):
    return {'code': code, 'severity': severity, 'count': count, 'detail': detail,
            'examples': [str(e)[:200] for e in list(examples)[:MAX_EXAMPLES]]}


SYNTHETIC_ADAPTERS = ('stub', 'scripted')


def _decided(r) -> bool:
    from dataclasses import asdict
    return r.juror.synthesis_status == 'full' and gates.synthesis_ok(asdict(r.juror))


def audit_result(res: CouncilResult) -> dict:
    h = health(res)
    out = []
    last: dict = {}
    for r in res.rounds:
        last[r.step_id] = r
    undecided = [f'{r.step_id}: {r.juror.synthesis_status}' + ('' if r.juror.synthesis_status != 'full' else
                 ' (recorded, but it fails the synthesis gate)') for r in last.values() if not _decided(r)]
    if undecided:
        out.append(_f('AUDIT-NO-SYNTHESIS', 'error', len(undecided),
                      f'{len(undecided)} of {len(last)} phases have no full synthesis', undecided))
    views = [v for r in res.rounds for v in r.views]
    off = [f'{r.step_id}/{v.stakeholder}' for r in res.rounds for v in r.views if v.status == 'offline']
    if off:
        sev = 'error' if 2 * len(off) >= max(1, len(views)) else 'warning'
        out.append(_f('AUDIT-OFFLINE-VIEWS', sev, len(off), f'{len(off)} of {len(views)} views are offline templates', off))
    failed = [f'{r.step_id}/{v.stakeholder}: {v.error}' for r in res.rounds for v in r.views if v.status == 'failed']
    if failed:
        out.append(_f('AUDIT-FAILED-VIEWS', 'warning', len(failed), f'{len(failed)} views were never valid', failed))
    fb = []
    for r in res.rounds:
        j = r.juror
        fields = [('merge_summary', j.merge_summary), ('decision', j.decision), ('spec_delta', j.spec_delta)]
        fields += [(f'{k}[{i}]', t) for k in ('resolved', 'next_steps', 'open_issues') for i, t in enumerate(getattr(j, k))]
        for name, t in fields:
            if gates.is_fallback(t):
                fb.append(f'{r.step_id}.{name}: {t[:80]}')
    if fb:
        out.append(_f('AUDIT-FALLBACK-TEXT', 'error', len(fb), 'fallback text where a decision should be', fb))
    # the same devil's-advocate or contribution text in several phases: a template
    by_text: dict = {}
    for r in res.rounds:
        for v in r.views:
            for t in (v.devils_advocate, v.contribution):
                key = (v.stakeholder, gates.squash(t))
                if len(key[1]) >= 20:
                    by_text.setdefault(key, set()).add(r.step_id.split('.')[0])
    tmpl = [f'{k[0]}: {k[1][:60]}' for k, steps in by_text.items() if len(steps) >= 3]
    if tmpl:
        out.append(_f('AUDIT-TEMPLATE-VIEWS', 'error', len(tmpl),
                      'the same view text appears in 3 or more phases', tmpl))
    steps = [s for r in last.values() for s in r.juror.next_steps]
    gen = [s for s in steps if gates.is_generic(s)]
    if gen:
        sev = 'error' if len(gen) == len(steps) else 'warning'
        out.append(_f('AUDIT-GENERIC-STEPS', sev, len(gen), f'{len(gen)} of {len(steps)} next steps name nothing', gen))
    ph = [f'{r.step_id}/{v.stakeholder}' for r in res.rounds for v in r.views if gates.has_placeholder_code(v.contribution)]
    ph += [f'{r.step_id}.spec_delta' for r in res.rounds if gates.has_placeholder_code(r.juror.spec_delta)]
    if ph:
        out.append(_f('AUDIT-PLACEHOLDER-CODE', 'warning', len(ph), 'placeholder code presented as content', ph))
    if res.flags:
        out.append(_f('AUDIT-UNSOURCED', 'warning', len(res.flags), 'figures without a source, or unknown sources',
                      [f"{x.get('step_id')}/{x.get('stakeholder')}: {x.get('figure') or x.get('source')}" for x in res.flags]))
    text = ' '.join([res.idea] + [t for r in res.rounds for t in (r.juror.merge_summary, r.juror.decision, r.juror.spec_delta,
                                                                    *[v.contribution for v in r.views])])
    comp = gates.compliance_block(text)
    if comp:
        out.append(_f('AUDIT-COMPLIANCE', 'error', len(comp), 'compliance rules fired', comp))
    if res.synthetic or res.adapter in SYNTHETIC_ADAPTERS or res.adapter.startswith('demo'):
        out.append(_f('AUDIT-SYNTHETIC', 'error', 1, f'adapter {res.adapter} is synthetic: not a real deliberation'))
    return _report('council', h, out)


def audit_stories(doc: dict) -> dict:
    stories = doc.get('stories', []) if isinstance(doc, dict) else []
    titles = [s.get('title', '') for s in stories if isinstance(s, dict)]
    gen = [t for t in titles if gates.is_generic(t)]
    blocked = [s.get('story_id', '') for s in stories if isinstance(s, dict) and s.get('status') == 'blocked']
    out = []
    if gen:
        sev = 'error' if len(gen) == len(titles) else 'warning'
        out.append(_f('AUDIT-GENERIC-STEPS', sev, len(gen), f'{len(gen)} of {len(titles)} story titles name nothing', gen))
    if blocked:
        out.append(_f('AUDIT-NO-SYNTHESIS', 'error', len(blocked), f'{len(blocked)} stories are blocked (no decision)', blocked))
    if not titles:
        out.append(_f('AUDIT-NO-SYNTHESIS', 'error', 0, 'no stories'))
    return _report('stories', {'stories': len(titles)}, out)


def _row_text(row) -> str:
    if not isinstance(row, dict):
        return ''
    if isinstance(row.get('messages'), list):
        return ' '.join(str(m.get('content', '')) for m in row['messages'] if isinstance(m, dict) and m.get('role') == 'assistant')
    return str(row.get('output', ''))


def audit_trainset(rows: list) -> dict:
    bad = []
    unreadable = []
    kinds: dict = {}
    for i, row in enumerate(rows):
        if not isinstance(row, dict) or not (isinstance(row.get('output'), str) or isinstance(row.get('messages'), list)):
            unreadable.append(f'row {i}: not an object with an output or messages')
            continue
        meta = row.get('meta', {})
        k = meta.get('kind') if isinstance(meta, dict) else None
        k = k if isinstance(k, str) else 'unknown'
        kinds[k] = kinds.get(k, 0) + 1
        t = _row_text(row)
        why = [n for n, test in (('fallback', gates.is_fallback(t)), ('placeholder code', gates.has_placeholder_code(t)))
               if test]
        if why:
            bad.append(f'row {i} ({k}): {", ".join(why)}')
    out = []
    if unreadable:
        out.append(_f('AUDIT-TRAINSET-UNREADABLE', 'error', len(unreadable),
                      f'{len(unreadable)} of {len(rows)} rows cannot be used', unreadable))
    if bad:
        out.append(_f('AUDIT-TRAINSET-CONTAMINATED', 'error', len(bad),
                      f'{len(bad)} of {len(rows)} rows teach fallback or placeholder output', bad))
    if not rows:
        out.append(_f('AUDIT-TRAINSET-CONTAMINATED', 'error', 0, 'no rows'))
    return _report('trainset', {'rows': len(rows), 'kinds': [[k, v] for k, v in sorted(kinds.items())]}, out)


def _report(kind, summary, findings) -> dict:
    order = {'error': 0, 'warning': 1, 'info': 2}
    findings.sort(key=lambda f: (order[f['severity']], f['code']))
    errors = sum(1 for f in findings if f['severity'] == 'error')
    return {'kind': kind, 'summary': summary, 'findings': findings,
            'verdict': 'build-ready' if errors == 0 else 'not build-ready'}


MAX_ROWS = 200_000             # rows read from all members together
MAX_MEMBERS = 1000
MAX_BYTES = 256 * 1024 * 1024  # uncompressed bytes read from all members together
MAX_LINE = 1024 * 1024         # longer lines count as unreadable rows
MAX_LINES = 1_000_000          # lines read from all members together, empty ones included
MAX_MEMBER = 64 * 1024 * 1024  # uncompressed bytes of one member


def read_jsonl(lines, budget=None) -> list:
    """Parsed rows (``None`` for a line that is not JSON), stopping at the row and byte budgets."""
    budget = budget if budget is not None else [MAX_ROWS, MAX_BYTES, MAX_LINES]
    rows = []
    for line in lines:
        budget[1] -= len(line)
        budget[2] -= 1
        if budget[1] < 0 or budget[0] <= 0 or budget[2] < 0:
            break
        line = line.strip(' \t\r\n')
        if not line:
            continue
        budget[0] -= 1
        if len(line) > MAX_LINE:
            rows.append(None)
            continue
        try:
            rows.append(json.loads(line))
        except (ValueError, RecursionError):
            rows.append(None)
    return rows


def audit_path(path: str) -> dict:
    """Audit one file: a council JSON (this package or SmartSignal), a stories JSON, a .jsonl trainset,
    or a .zip holding .jsonl files (only *.jsonl members are read; each at most 64 MiB)."""
    from .importer import detect, from_smartsignal
    from .model import result_from_dict
    from .schema import check_council
    if path.endswith('.zip'):
        rows: list = []
        budget = [MAX_ROWS, MAX_BYTES, MAX_LINES]
        try:
            with zipfile.ZipFile(path) as z:
                members = [i for i in z.infolist() if i.filename.endswith('.jsonl') and 'flagged' not in i.filename
                           and not i.is_dir()]
                if len(members) > MAX_MEMBERS:
                    raise ValueError(f'more than {MAX_MEMBERS} .jsonl members')
                for info in members:
                    if info.file_size > MAX_MEMBER:
                        raise ValueError(f'{info.filename}: larger than 64 MiB uncompressed')
                    with z.open(info) as f:
                        text = io.TextIOWrapper(f, encoding='utf-8', errors='replace')
                        rows += read_jsonl(text, budget)
                    if budget[0] <= 0 or budget[1] < 0 or budget[2] < 0:
                        break
        except (zipfile.BadZipFile, RuntimeError, NotImplementedError, EOFError, OSError) as e:
            raise ValueError(f'cannot read the zip: {e}') from None
        return audit_trainset(rows)
    if path.endswith('.jsonl'):
        with open(path, encoding='utf-8', errors='replace') as f:
            return audit_trainset(read_jsonl(f))
    with open(path, encoding='utf-8') as f:
        try:
            doc = json.load(f)
        except RecursionError:
            raise ValueError('JSON nested too deeply') from None
    if isinstance(doc, list) and doc and all(isinstance(r, dict) and 'views' in r for r in doc):
        doc = {'rounds': doc}                    # a bare list of rounds (a council pack's rounds.json)
    kind = detect(doc)
    if kind == 'smartsavant':
        errs = check_council(doc)
        if errs:
            raise ValueError('not a valid council file: ' + '; '.join(errs[:3]))
        return audit_result(result_from_dict(doc))
    if kind:
        rep = audit_result(from_smartsignal(doc))
        rep['source'] = kind
        return rep
    if isinstance(doc, dict) and isinstance(doc.get('stories'), list):
        return audit_stories(doc)
    raise ValueError('not a council result, a council export, a stories file or a trainset')


_ESC = {c: f'\\x{c:02x}' for c in list(range(32)) + list(range(127, 160))}
_ESC.update({c: f'\\u{c:04x}' for c in list(range(0x202A, 0x202F)) + list(range(0x2066, 0x206A))})
_ESC.update({c: f'\\u{c:04x}' for c in range(0xD800, 0xE000)})


def esc(text) -> str:
    """Text safe for one terminal line: control characters (newlines included), bidirectional overrides
    and unpaired surrogates shown as escapes."""
    return str(text).translate(_ESC)


def render(rep: dict, name: str = '') -> str:
    lines = [f'{esc(name)}: {rep["verdict"].upper()} ({rep["kind"]}{", " + rep["source"] if rep.get("source") else ""})',
             f'  {json.dumps(rep["summary"], sort_keys=True)}']
    for f in rep['findings']:
        lines.append(f'  [{f["severity"]}] {f["code"]}: {esc(f["detail"])}')
        for e in f['examples']:
            lines.append(f'      - {esc(e)}')
    return '\n'.join(lines)
