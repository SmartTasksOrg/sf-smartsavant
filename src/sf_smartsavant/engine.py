"""The council engine: roster -> phases -> stakeholder views -> juror merge -> decision tree.

    res = Council(idea="...", adapter=StubAdapter()).run()

For every phase (spec/phases.json) and iteration, each stakeholder's view is requested, validated
(schema.check_view) and on failure retried ``retries`` times; a view that still fails is recorded with
status ``failed`` and an error, never filled with invented text. The juror merges the valid views; its
answer is validated the same way and gets a ``synthesis_status``: ``full`` when it passes
gates.synthesis_ok, ``thin`` when it is valid but decides nothing usable, ``failed`` when it never
became valid, ``no_views`` when there was nothing to merge. A phase repeats while the juror recommends
it, up to ``max_loops`` iterations. Resolved entries accumulate as locked decisions that later phases
see in their context. Deterministic for a deterministic adapter (spec/conformance/runs.json).
"""
from __future__ import annotations

import copy
import json
import re
from dataclasses import asdict

from . import gates
from .adapters.base import LLMAdapter
from .model import CouncilResult, CouncilRound, DecisionNode, JurorMerge, StakeholderView, W5H1
from .schema import NotPlainData, check_merge, check_view, normalize_view, plain

DEFAULT_ROSTER = ['Developer', 'UX/UI', 'Product Owner', 'Architecture/Infra']


def load_phases(path: str | None = None) -> list:
    """[(step_id, title, question)] from a phases file (default: the one shipped in the package)."""
    if path is None:
        from ._bundled import read_text
        doc = json.loads(read_text('phases.json'))
    else:
        with open(path, encoding='utf-8') as f:
            doc = json.load(f)
    ph = doc.get('phases') if isinstance(doc, dict) else None
    if not isinstance(ph, list) or not ph:
        raise ValueError('a phases file is {"phases": [{"step_id", "title", "question"}, ...]}')
    if not all(isinstance(p, dict) for p in ph):
        raise ValueError('each phase is an object {"step_id", "title", "question"}')
    return check_phases([(p.get('step_id'), p.get('title'), p.get('question')) for p in ph])


DEFAULT_PHASES = [
    ('frame', 'Frame', 'What is the problem, who are the users, and the jobs-to-be-done?'),
    ('concept', 'Concept', 'What is the solution direction and the wedge?'),
    ('design', 'Design', 'What are the user flow, data flow, and experience?'),
    ('build', 'Build', 'What is the technical approach?'),
    ('integrate', 'Integrate', 'How does it fit the target architecture?'),
    ('measure', 'Measure', 'What are the KPIs and success metrics?'),
    ('spec', 'Spec', 'Assemble the final document.'),
]


_SURROGATE = re.compile('[\ud800-\udfff]')


def _cut(text: str, n: int) -> str:
    return text[:n]


def _error(e: BaseException) -> str:
    """``Name: message`` for an exception, secrets redacted, at most 500 code points. Never raises."""
    try:
        msg = str(e)
        if type(msg) is not str:
            raise TypeError
    except Exception:
        msg = '(message not printable)'
    msg = msg.encode('utf-8', 'replace').decode('utf-8')          # no unpaired surrogates in records
    return _cut(gates.redact(f'{type(e).__name__}: {msg}'), 500)


def check_phases(phases) -> list:
    """[(step_id, title, question)] with unique non-blank step ids and non-blank strings, or ValueError."""
    out, seen = [], set()
    for p in phases:
        if not isinstance(p, (tuple, list)) or len(p) != 3 or not all(type(x) is str and not gates.blank(x) for x in p):
            raise ValueError('each phase is (step_id, title, question), three non-blank strings')
        if p[0] in seen:
            raise ValueError(f'duplicate step_id {p[0]}')
        seen.add(p[0])
        out.append(tuple(p))
    if not out:
        raise ValueError('at least one phase')
    return out


class Council:
    def __init__(self, idea: str, adapter: LLMAdapter, roster: list | None = None, phases: list | None = None,
                 sources: list | None = None, max_loops: int = 1, retries: int = 1):
        if type(idea) is not str or gates.blank(idea):
            raise ValueError('the idea must be a non-empty string')
        texts = [idea, *(roster or []), *(sources or []), *(x for p in (phases or []) for x in (p if isinstance(p, (list, tuple)) else []))]
        if any(type(t) is str and _SURROGATE.search(t) for t in texts):
            raise ValueError('the idea, roster, phases and sources must not contain unpaired surrogates')
        if not isinstance(adapter, LLMAdapter):
            raise TypeError('adapter must be an LLMAdapter')
        self.roster = list(roster) if roster else list(DEFAULT_ROSTER)
        if len(set(self.roster)) != len(self.roster) or not all(type(r) is str and not gates.blank(r) for r in self.roster):
            raise ValueError('roster names must be unique non-empty strings')
        self.phases = check_phases(list(phases)) if phases else list(DEFAULT_PHASES)
        if not isinstance(max_loops, int) or not 1 <= max_loops <= 10:
            raise ValueError('max_loops is an integer from 1 to 10')
        if not isinstance(retries, int) or not 0 <= retries <= 5:
            raise ValueError('retries is an integer from 0 to 5')
        self.idea, self.adapter = idea, adapter
        self.sources = list(sources or [])
        self.max_loops, self.retries = max_loops, retries

    def _ask(self, call, check, kwargs):
        """(answer, '') or (None, error) after 1 + retries attempts."""
        err = ''
        for _ in range(1 + self.retries):
            try:
                ans = plain(call(**copy.deepcopy(kwargs)))   # validate exactly what will be used
            except NotPlainData as e:
                err = _cut(f'invalid: {e}', 500)
                continue
            except Exception as e:                       # an adapter failure is data, not a crash
                err = _error(e)
                continue
            problems = check(ans)
            if not problems:
                return ans, ''
            err = _cut('invalid: ' + '; '.join(_cut(p, 200) for p in problems[:5]), 500)
        return None, err

    def run(self) -> CouncilResult:
        res = CouncilResult(idea=self.idea, adapter=str(getattr(self.adapter, 'name', 'adapter')),
                            synthetic=bool(getattr(self.adapter, 'synthetic', False)),
                            stakeholders=list(self.roster), sources=list(self.sources))
        prior: list = []
        for step_id, title, question in self.phases:
            it = 0
            while True:
                it += 1
                ctx = {'idea': self.idea, 'iteration': it, 'sources': list(self.sources), 'prior': prior,
                       'locked': res.locked_decisions}
                views = []
                for role in self.roster:
                    raw, err = self._ask(self.adapter.view, check_view,
                                         dict(stakeholder=role, role=role, phase_id=step_id, question=question,
                                              context=ctx))
                    if raw is None:
                        views.append(StakeholderView(stakeholder=role, role=role, status='failed', error=err))
                        continue
                    raw = normalize_view(raw)
                    sv = StakeholderView(stakeholder=role, role=role, status='ok', w5h1=W5H1(**raw['w5h1']),
                                         done_so_far=raw['done_so_far'], missing=list(raw['missing']),
                                         devils_advocate=raw['devils_advocate'], contribution=raw['contribution'],
                                         sources=list(raw['sources']))
                    views.append(sv)
                    for fl in gates.flag_unsourced({**raw, 'stakeholder': role}, self.sources):
                        res.flags.append({**fl, 'step_id': step_id, 'iteration': it})
                ok_views = [asdict(v) for v in views if v.status == 'ok']
                if not ok_views:
                    juror = JurorMerge(synthesis_status='no_views', error='no valid view to merge')
                else:
                    m, err = self._ask(self.adapter.merge, check_merge,
                                       dict(phase_id=step_id, question=question, views=ok_views, context=ctx))
                    if m is None:
                        juror = JurorMerge(synthesis_status='failed', error=err)
                    else:
                        juror = JurorMerge(synthesis_status='full', merge_summary=m['merge_summary'],
                                           decision=m['decision'], resolved=list(m['resolved']),
                                           open_issues=list(m['open_issues']), next_steps=list(m['next_steps']),
                                           reversals=list(m.get('reversals', [])), spec_delta=m['spec_delta'],
                                           loop_recommended=m['loop_recommended'])
                        if not gates.synthesis_ok(asdict(juror)):
                            juror.synthesis_status = 'thin'
                rnd = CouncilRound(step_id=step_id, step_title=title, iteration=it, question=question,
                                   views=views, juror=juror)
                res.rounds.append(rnd)
                res.decision_tree.append(DecisionNode(step_id=step_id, iteration=it,
                                                      branches=[v.contribution for v in views if v.status == 'ok'],
                                                      decision=juror.decision, next_steps=list(juror.next_steps)))
                if juror.synthesis_status == 'full':
                    for r in juror.resolved:
                        if r not in res.locked_decisions:
                            res.locked_decisions.append(r)
                    prior = prior + [{'step': step_id, 'decision': juror.decision}]
                if not juror.loop_recommended or it >= self.max_loops:
                    break
        return res


def health(res: CouncilResult) -> dict:
    """Counts for a run: views by status, merges by synthesis status, flags, and whether every phase's
    last iteration reached a full synthesis (``decided``)."""
    views: dict = {}
    merges: dict = {}
    last: dict = {}
    for r in res.rounds:
        for v in r.views:
            views[v.status] = views.get(v.status, 0) + 1
        merges[r.juror.synthesis_status] = merges.get(r.juror.synthesis_status, 0) + 1
        last[r.step_id] = r.juror.synthesis_status
    return {'views': dict(sorted(views.items())), 'merges': dict(sorted(merges.items())),
            'phases': len(last), 'phases_decided': sum(1 for s in last.values() if s == 'full'),
            'decided': bool(last) and all(s == 'full' for s in last.values()),
            'flags': len(res.flags), 'synthetic': res.synthetic}


def load_adapter(spec: str, args: dict | None = None) -> LLMAdapter:
    """``stub``, ``replay:FILE`` or ``module:Class`` (imported from the Python path; your own code)."""
    if spec == 'stub':
        from .adapters import StubAdapter
        return StubAdapter()
    if spec.startswith('replay:'):
        from .adapters import ReplayAdapter
        return ReplayAdapter(spec[7:])
    if ':' not in spec:
        raise ValueError('adapter is stub, replay:FILE or module:Class')
    import importlib
    mod, name = spec.split(':', 1)
    cls = getattr(importlib.import_module(mod), name)
    if not (isinstance(cls, type) and issubclass(cls, LLMAdapter)):     # checked before anything is called
        raise TypeError(f'{spec} is not an LLMAdapter class')
    return cls(**(args or {}))
