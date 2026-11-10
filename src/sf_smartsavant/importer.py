"""Read council exports written by other tools (the SmartSignal council) into a CouncilResult, so that
the gates, the audit and the exporters work on them.

Two shapes are recognised (spec/SEMANTICS.md, "Imports"):
- a council result: ``{"idea", "stakeholders", "rounds": [{"step_id", "step_title", "iteration", "views":
  [{stakeholder, role, w5h1, done_so_far, missing, devils_advocate, contribution, source, ...}], "juror":
  {merge_summary, resolved, open_issues, next_steps, spec_delta, synthesis_status, ...}}], "decisions"}``;
- a rounds export: ``{"council_build", "rounds": [{"step_id", "step_title", "round", "views": [{stakeholder,
  role, source, parsed: {...}}], "juror": {...}}]}``.

Mapping: a view whose ``source`` is ``offline`` becomes status ``offline`` (a template, not a model);
others become ``ok``. A juror status ``full``/``recovered``/``offline``/``failed`` is kept, except that a
``full`` merge that fails gates.synthesis_ok becomes ``thin`` (with a note in ``error``). An object
``contribution`` is flattened to text ("key: value" lines). When a step id repeats with the same
iteration (an export that holds two runs), later occurrences get ``.2``, ``.3``, ... appended.
"""
from __future__ import annotations

import json
import re

from . import gates
from .model import CouncilResult, CouncilRound, DecisionNode, JurorMerge, StakeholderView, W5H1

_SURROGATE = re.compile('[\ud800-\udfff]')

_STATUS = {'full', 'recovered', 'offline', 'failed', 'thin', 'no_views'}


MAX_DEPTH = 20


def _clean(t: str) -> str:
    """Unpaired surrogates (which cannot be written as UTF-8) become U+FFFD."""
    return _SURROGATE.sub('\ufffd', t)


def _text(v, depth=0) -> str:
    if depth > MAX_DEPTH:
        return '[nested too deep]'
    if v is None:
        return ''
    if isinstance(v, str):
        return _clean(v)
    if isinstance(v, dict):
        return '\n'.join(f'{_text(k, depth + 1)}: {_text(x, depth + 1)}' for k, x in v.items())
    if isinstance(v, list):
        return '; '.join(_text(x, depth + 1) for x in v)
    if isinstance(v, bool):
        return 'true' if v else 'false'
    if isinstance(v, int):
        return str(v)
    return _clean(json.dumps(v, ensure_ascii=False)) if isinstance(v, float) and v == v and abs(v) != float('inf') else str(type(v).__name__)


def _dict(v) -> dict:
    return v if isinstance(v, dict) else {}


def _list(v) -> list:
    return v if isinstance(v, list) else []


def _strs(v) -> list:
    if v is None:
        return []
    if isinstance(v, str):
        return [] if gates.blank(v) else [v]
    if isinstance(v, list):
        return [_text(x) for x in v if not gates.blank(_text(x))]
    return [_text(v)]


def idea_text(idea) -> str:
    if isinstance(idea, str):
        return idea
    if isinstance(idea, dict):
        name, prob, sol, aud = (_text(idea.get(k, '')) for k in ('name', 'problem', 'solution', 'audience'))
        s = f'{name}: {prob}'.strip(': ')
        if sol:
            s += f' -> {sol}'
        if aud:
            s += f' (audience: {aud})'
        return s
    return _text(idea)


def detect(doc) -> str | None:
    if not isinstance(doc, dict):
        return None
    if doc.get('schema') == 'smartsavant.council/1':
        return 'smartsavant'
    rounds = doc.get('rounds')
    if isinstance(rounds, list) and rounds and any(isinstance(r, dict) and 'views' in r for r in rounds):
        for r in rounds:
            for v in _list(_dict(r).get('views')):
                if isinstance(v, dict):
                    return 'smartsignal-rounds' if 'parsed' in v else 'smartsignal-result'
        return 'smartsignal-result'
    return None


def from_smartsignal(doc: dict) -> CouncilResult:
    kind = detect(doc)
    if kind not in ('smartsignal-rounds', 'smartsignal-result'):
        raise ValueError('not a SmartSignal council export (no rounds with views)')
    models = set()
    res = CouncilResult(idea=_clean(idea_text(doc.get('idea', ''))), synthetic=False)
    seen: dict = {}
    names: list = []
    for r in doc['rounds']:
        if not isinstance(r, dict):
            continue
        step = _text(r.get('step_id', 'step')) or 'step'
        it, rnd = r.get('iteration'), r.get('round')
        it = (it + 1 if type(it) is int and 0 <= it < 1000 else
              rnd + 1 if type(rnd) is int and 0 <= rnd < 1000 else 1)
        key = (step, it)
        seen[key] = seen.get(key, 0) + 1
        if seen[key] > 1:
            step = f'{step}.{seen[key]}'
        views = []
        for v in _list(r.get('views')):
            if not isinstance(v, dict):
                continue
            p = _dict(v.get('parsed', v) if kind == 'smartsignal-rounds' else v)
            src = _text(v.get('source', ''))
            if v.get('model'):
                models.add(_text(v['model']))
            w = _dict(p.get('w5h1'))
            who = _text(v.get('stakeholder', ''))
            if who not in names:
                names.append(who)
            views.append(StakeholderView(
                stakeholder=who, role=_text(v.get('role', '')), status='offline' if src == 'offline' else 'ok',
                w5h1=W5H1(**{k: _text(w.get(k, '')) for k in ('who', 'what', 'where', 'when', 'why', 'how')}),
                done_so_far=_text(p.get('done_so_far')), missing=_strs(p.get('missing')),
                devils_advocate=_text(p.get('devils_advocate')), contribution=_text(p.get('contribution')),
                sources=[s for s in _strs(p.get('sources')) if len(s) <= 200]))
        j = _dict(r.get('juror'))
        if j.get('model'):
            models.add(_text(j['model']))
        st = _text(j.get('synthesis_status', '')) or 'failed'
        st = st if st in _STATUS else 'failed'
        resolved = _strs(j.get('resolved'))
        juror = JurorMerge(synthesis_status=st, merge_summary=_text(j.get('merge_summary')),
                           decision=_text(j.get('decision')) or (resolved[0] if resolved else ''),
                           resolved=resolved, open_issues=_strs(j.get('open_issues')),
                           next_steps=_strs(j.get('next_steps')), reversals=_strs(j.get('reversals')),
                           spec_delta=_text(j.get('spec_delta')),
                           loop_recommended=j.get('loop_recommended') is True)
        if st == 'full':
            from dataclasses import asdict
            if not gates.synthesis_ok(asdict(juror)):
                juror.synthesis_status = 'thin'
                juror.error = 'imported as full, but it fails the synthesis gate'
        res.rounds.append(CouncilRound(step_id=step, step_title=_text(r.get('step_title', step)) or step, iteration=it,
                                       question=_text(r.get('question', r.get('step_title', ''))), views=views,
                                       juror=juror))
        res.decision_tree.append(DecisionNode(step_id=step, iteration=it,
                                              branches=[x.contribution for x in views if x.status == 'ok'],
                                              decision=juror.decision, next_steps=list(juror.next_steps)))
        for x in views:
            if x.status == 'ok':
                for fl in gates.flag_unsourced({'stakeholder': x.stakeholder, 'contribution': x.contribution,
                                                'devils_advocate': x.devils_advocate, 'done_so_far': x.done_so_far,
                                                'missing': x.missing, 'sources': x.sources}, []):
                    res.flags.append({**fl, 'step_id': step, 'iteration': it})
        if juror.synthesis_status == 'full':
            for d in juror.resolved:
                if d not in res.locked_decisions:
                    res.locked_decisions.append(d)
    dec = doc.get('decisions')
    any_full = any(r.juror.synthesis_status == 'full' for r in res.rounds)
    if any_full and isinstance(dec, list) and dec and all(isinstance(d, dict) for d in dec):
        res.locked_decisions = [x for x in (f"{_text(d.get('topic', ''))}: {_text(d.get('decision', ''))}" for d in dec)
                                if not gates.is_fallback(x)]
    res.stakeholders = [_text(s.get('name', '')) if isinstance(s, dict) else _text(s)
                        for s in _list(doc.get('stakeholders'))] or names
    res.adapter = 'smartsignal:' + ','.join(sorted(models)) if models else 'smartsignal'
    return res
