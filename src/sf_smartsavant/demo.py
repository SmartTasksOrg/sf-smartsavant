"""The built-in demo: three councils in memory, no model, no network, no files.

1. a stub council (deterministic): every phase decided and exported; the audit refuses it only because
   it is synthetic (a stub is never a basis for building);
2. a council whose adapter breaks (invalid answers, an exception): the failures are recorded, nothing is
   invented, the stories are blocked and the audit says "not build-ready";
3. the same broken run re-exported from its own council.json via the replay adapter.
"""
from __future__ import annotations

import json

from .adapters import LLMAdapter, ReplayAdapter, StubAdapter
from .audit import audit_result, render
from .engine import Council, health
from .exporters import to_spec_md, to_stories, to_trainset

IDEA = 'Learn reusable automations from recorded operator sessions'


class Flaky(StubAdapter):
    """Breaks on purpose: no valid view from 'Product Owner', a juror that raises at 'build', and a juror
    that answers fallback text at 'measure'."""
    name = 'demo-flaky'
    synthetic = True

    def view(self, **kw):
        if kw['stakeholder'] == 'Product Owner':
            return {'w5h1': {}, 'contribution': 42}
        return super().view(**kw)

    def merge(self, **kw):
        if kw['phase_id'] == 'build':
            raise TimeoutError('the model did not answer in 60 s')
        m = super().merge(**kw)
        if kw['phase_id'] == 'measure':
            m.update(merge_summary='Juror synthesis failed; falling back to the raw panel contributions.',
                     next_steps=['Advance the measure with the decided approach'])
        return m


def run(out=print) -> int:
    ok = True
    out(f'idea: {IDEA}')
    out('1. stub council (deterministic, synthetic)')
    res = Council(IDEA, StubAdapter()).run()
    h = health(res)
    out(f'   {h["phases_decided"]}/{h["phases"]} phases decided, views {h["views"]}, merges {h["merges"]}')
    st = to_stories(res, 'DEMO')
    out(f'   stories: {st["counts"]}; trainset rows: {len(to_trainset(res))} (methodology only: the run is synthetic)')
    rep = audit_result(res)
    out('   ' + render(rep, 'audit').replace('\n', '\n   '))
    ok &= h['decided'] and [f['code'] for f in rep['findings']] == ['AUDIT-SYNTHETIC']

    out('2. a council whose adapter breaks')
    bad = Council(IDEA, Flaky(), retries=1).run()
    hb = health(bad)
    out(f'   {hb["phases_decided"]}/{hb["phases"]} phases decided, views {hb["views"]}, merges {hb["merges"]}')
    sb = to_stories(bad, 'DEMO')
    for s in sb['stories']:
        if s['status'] == 'blocked':
            out(f'   blocked: {s["title"]}')
    rb = audit_result(bad)
    out('   ' + render(rb, 'audit').replace('\n', '\n   '))
    ok &= not hb['decided'] and rb['verdict'] == 'not build-ready' and sb['counts']['blocked'] == 2
    ok &= 'NOT DECIDED' in to_spec_md(bad)

    out('3. replay of run 2 from its council.json')
    rec = json.loads(json.dumps(bad.to_dict()))
    again = Council(IDEA, ReplayAdapter(rec), retries=0).run()
    same = health(again)['merges'] == hb['merges'] and [r.juror.decision for r in again.rounds] == [r.juror.decision for r in bad.rounds]
    out(f'   same decisions and the same health: {same}')
    ok &= same
    out('demo OK' if ok else 'demo FAILED')
    return 0 if ok else 1
