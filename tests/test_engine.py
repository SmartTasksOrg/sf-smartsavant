import pytest

from sf_smartsavant import Council, ReplayAdapter, StubAdapter, health
from sf_smartsavant.adapters import LLMAdapter, ScriptedAdapter
from sf_smartsavant.exporters import run_digest, to_spec_md, to_stories, to_trainset
from sf_smartsavant.model import result_from_dict
from sf_smartsavant.schema import check_council

IDEA = 'Learn reusable automations from recorded operator sessions'


def test_deterministic_and_round_trips():
    a, b = Council(IDEA, StubAdapter()).run(), Council(IDEA, StubAdapter()).run()
    assert a.to_dict() == b.to_dict() and run_digest(a) == run_digest(b)
    assert check_council(a.to_dict()) == []
    assert result_from_dict(a.to_dict()).to_dict() == a.to_dict()


def test_bad_arguments_are_refused():
    for kw in ({'idea': ' '}, {'roster': ['a', 'a']}, {'max_loops': 0}, {'retries': 9}):
        args = {'idea': IDEA, 'adapter': StubAdapter(), **kw}
        with pytest.raises(ValueError):
            Council(**args)
    with pytest.raises(TypeError):
        Council(IDEA, object())


class Mutating(StubAdapter):
    """Tries to change the engine's state through the arguments it receives."""

    def view(self, **kw):
        kw['context']['locked'].append('injected: yes')
        kw['context']['prior'].append({'step': 'x', 'decision': 'y'})
        return super().view(**kw)

    def merge(self, **kw):
        for v in kw['views']:
            v['contribution'] = 'rewritten'
        return super().merge(**kw)


def test_adapters_cannot_mutate_the_run():
    res = Council(IDEA, Mutating()).run()
    assert 'injected: yes' not in res.locked_decisions
    assert all(v.contribution != 'rewritten' for r in res.rounds for v in r.views)


class Counting(LLMAdapter):
    name = 'counting'

    def __init__(self):
        self.calls = 0

    def view(self, **kw):
        self.calls += 1
        raise RuntimeError('down')

    def merge(self, **kw):
        raise AssertionError('never called without views')


def test_retries_and_no_views():
    a = Counting()
    res = Council(IDEA, a, roster=['A', 'B'], phases=[('p', 'P', 'q?')], retries=2).run()
    assert a.calls == 6
    assert res.rounds[0].juror.synthesis_status == 'no_views'
    assert res.rounds[0].views[0].error == 'RuntimeError: down'
    assert not health(res)['decided']


def test_loops_stop_at_max_loops():
    m = {'merge_summary': 's', 'decision': 'd', 'resolved': ['r: x'], 'open_issues': [], 'next_steps': ['Write the x'],
         'spec_delta': '', 'loop_recommended': True}
    res = Council(IDEA, ScriptedAdapter({'merges': {f'p|{i}': [m] for i in range(1, 6)}}), phases=[('p', 'P', 'q?')],
                  max_loops=3).run()
    assert [r.iteration for r in res.rounds] == [1, 2, 3]
    assert res.locked_decisions == ['r: x']


def test_replay_reproduces_a_recorded_run():
    script = {'views': {'build|1|UX/UI': [{'raise': 'x'}]}, 'merges': {'measure|1': [{'raise': 'y'}]}}
    first = Council(IDEA, ScriptedAdapter(script), retries=0).run()
    again = Council(IDEA, ReplayAdapter(first.to_dict()), retries=0).run()
    assert [r.juror.synthesis_status for r in again.rounds] == [r.juror.synthesis_status for r in first.rounds]
    assert [r.juror.decision for r in again.rounds] == [r.juror.decision for r in first.rounds]


def test_exports_never_hide_an_undecided_phase():
    res = Council(IDEA, ScriptedAdapter({'merges': {'build|1': [{'raise': 'model down'}]}}), retries=0).run()
    st = to_stories(res, 'T')
    blocked = [s for s in st['stories'] if s['status'] == 'blocked']
    assert len(blocked) == 1 and 'did not reach a decision (failed)' in blocked[0]['title']
    assert 'AdapterError: model down' in blocked[0]['open_issues']
    md = to_spec_md(res)
    assert '**NOT DECIDED** (failed)' in md and 'Not build-ready' in md


def test_trainset_never_learns_from_synthetic_runs():
    res = Council(IDEA, StubAdapter()).run()
    assert {r['meta']['kind'] for r in to_trainset(res)} == {'methodology'}
    assert {r['meta']['kind'] for r in to_trainset(res, allow_synthetic=True)} == {'methodology', 'expert', 'facilitation'}


def test_model_text_is_inert_in_markdown():
    m = {'merge_summary': '<img src=x onerror=alert(1)>\n# not a heading', 'decision': 'd', 'resolved': ['a|b'],
         'open_issues': [], 'next_steps': ['Write it'], 'spec_delta': '', 'loop_recommended': False}
    res = Council('<b>idea</b>', ScriptedAdapter({'merges': {'p|1': [m]}}), phases=[('p', 'P', 'q?')]).run()
    md = to_spec_md(res)
    assert '<img' not in md and '<b>' not in md and '\n# not a heading' not in md
    assert '| 1 | a\\|b |' in md
