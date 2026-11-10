"""Regression tests for the findings of the pre-release review and the private robustness suite."""
import json
import os
import subprocess
import sys
import zipfile

import pytest

from sf_smartsavant import Council, StubAdapter, gates
from sf_smartsavant.adapters import LLMAdapter, ScriptedAdapter
from sf_smartsavant.audit import audit_path, audit_result, audit_trainset
from sf_smartsavant.exporters import run_digest, to_mermaid, to_spec_md, to_stories, to_trainset
from sf_smartsavant.importer import from_smartsignal
from sf_smartsavant.model import result_from_dict
from sf_smartsavant.schema import check_council

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIX = json.load(open(os.path.join(ROOT, 'spec', 'conformance', 'audit.json')))['cases']
GOOD_MERGE = {'merge_summary': 's', 'decision': 'd', 'resolved': ['r: x'], 'open_issues': [], 'next_steps': ['Write the x'],
              'spec_delta': '', 'loop_recommended': False}
PH = [('p', 'P', 'q?')]


def cli(*args):
    env = dict(os.environ, PYTHONPATH=os.path.join(ROOT, 'src'))
    return subprocess.run([sys.executable, '-m', 'sf_smartsavant', *args], capture_output=True, text=True, env=env)


# 1: fallback text anywhere in a merge is never a full synthesis
def test_fallback_in_resolved_or_steps_is_not_full():
    m = dict(GOOD_MERGE, resolved=['Juror synthesis failed; falling back to the raw panel contributions.'],
             next_steps=['Juror returned unparseable output (offline fallback)', 'Write the module'])
    res = Council('x', ScriptedAdapter({'merges': {'p|1': [m]}}), phases=PH).run()
    assert res.rounds[0].juror.synthesis_status == 'thin'
    assert gates.is_generic('Juror returned unparseable output (offline fallback)')
    codes = {f['code'] for f in audit_result(res)['findings']}
    assert 'AUDIT-FALLBACK-TEXT' in codes and 'AUDIT-NO-SYNTHESIS' in codes


# 2: the audit does not take a council file at its word
def test_audit_recomputes_recorded_status_and_synthetic_names():
    d = Council('x', StubAdapter(), phases=PH).run().to_dict()
    d['synthetic'] = False
    d['rounds'][0]['juror'].update(decision='', resolved=[], next_steps=[])
    rep = audit_result(result_from_dict(d))
    codes = [f['code'] for f in rep['findings']]
    assert 'AUDIT-NO-SYNTHESIS' in codes and 'AUDIT-SYNTHETIC' in codes and rep['verdict'] == 'not build-ready'


def test_garbage_trainset_is_not_build_ready():
    rep = audit_trainset([None, [1], 'x', {}, {'output': 'fine', 'meta': {'kind': {'not': 'a string'}}}])
    assert rep['verdict'] == 'not build-ready'
    assert rep['findings'][0]['code'] == 'AUDIT-TRAINSET-UNREADABLE' and rep['findings'][0]['count'] == 4
    assert rep['summary']['kinds'] == [['unknown', 1]]


def test_check_council_checks_types():
    d = Council('x', StubAdapter(), phases=PH).run().to_dict()
    assert check_council(d) == []
    for path, val in ((('rounds', 0, 'step_id'), 5), (('rounds', 0, 'iteration'), 1.0), (('flags',), [5]),
                      (('rounds', 0, 'views', 0, 'missing'), 'x'), (('idea',), '\ud800')):
        bad = json.loads(json.dumps(d))
        t = bad
        for k in path[:-1]:
            t = t[k]
        t[path[-1]] = val
        assert check_council(bad), path


# 3: subclasses and getters cannot smuggle invalid content past validation
class LyingDict(dict):
    def __getitem__(self, k):
        return 'valid text'


class ShortStr(str):
    def __len__(self):
        return 1


class Liar(StubAdapter):
    def view(self, **kw):
        v = super().view(**kw)
        v['devils_advocate'] = 12345
        return LyingDict(v)

    def merge(self, **kw):
        return dict(GOOD_MERGE, spec_delta=ShortStr('x' * 30000))


def test_subclasses_are_refused():
    res = Council('x', Liar(), phases=PH, retries=0).run()
    assert all(v.status == 'failed' and 'not JSON data' in v.error for v in res.rounds[0].views)
    assert res.rounds[0].juror.synthesis_status == 'no_views'
    res = Council('x', type('M', (StubAdapter,), {'merge': Liar.merge})(), phases=PH, retries=0).run()
    assert res.rounds[0].juror.synthesis_status == 'failed'


# 4: unpaired surrogates never reach records, digests or files
def test_surrogates_are_refused_and_cleaned_on_import(tmp_path):
    bad = json.loads('"\\ud83d"')
    res = Council('x', ScriptedAdapter({'views': {'p|1|Developer': [dict(StubAdapter().view(
        stakeholder='Developer', role='Developer', phase_id='p', question='q', context={'idea': 'x'}), contribution=bad)]}}),
        phases=PH, retries=0).run()
    assert res.rounds[0].views[0].status == 'failed' and 'unpaired surrogate' in res.rounds[0].views[0].error
    run_digest(res)
    doc = json.loads(json.dumps(FIX['import'][0]['input']))
    doc['rounds'][0]['juror']['merge_summary'] = bad
    imp = from_smartsignal(doc)
    run_digest(imp) and to_spec_md(imp)
    p = tmp_path / 'e.json'
    p.write_text(json.dumps(doc))
    assert cli('import', str(p), '--out', str(tmp_path / 'c.json')).returncode == 0
    assert cli('audit', str(p)).returncode in (0, 1)


# 5: model text cannot inject Markdown
def test_markdown_injection_is_inert():
    m = dict(GOOD_MERGE, merge_summary='```', decision='# Approved by security', resolved=['![x](https://e.example/p)'],
             next_steps=['[click](javascript:alert(1))'])
    res = Council('x', ScriptedAdapter({'merges': {'p|1': [m]}}), phases=PH).run()
    md = to_spec_md(res)
    assert '\n```' not in md and '\n# Approved' not in md and '![x](' not in md and '[click](' not in md
    assert '\\# Approved by security' in md and '\\`\\`\\`' in md


# 6, 7: unreadable inputs are exit 2, and zips are bounded
def test_unreadable_inputs_exit_2(tmp_path):
    cases = {'a.json': '{"rounds":[{"views":[]},5]}', 'b.json': '[' * 100000, 'c.zip': 'not a zip',
             'd.json': '{"rounds":[{"views":"abc","juror":[1]}]}'}
    for name, body in cases.items():
        p = tmp_path / name
        p.write_text(body)
        r = cli('audit', str(p))
        assert r.returncode in (1, 2) and 'Traceback' not in r.stderr, (name, r.stderr)
    r = cli('validate', str(tmp_path / 'b.json'))
    assert r.returncode == 1 and 'Traceback' not in r.stderr


def test_zip_total_rows_are_capped(tmp_path, monkeypatch):
    import sf_smartsavant.audit as A
    monkeypatch.setattr(A, 'MAX_ROWS', 50)
    z = tmp_path / 'many.zip'
    with zipfile.ZipFile(z, 'w', zipfile.ZIP_DEFLATED) as zf:
        for i in range(20):
            zf.writestr(f'm{i}.jsonl', '{"output": "x"}\n' * 100)
    assert audit_path(str(z))['summary']['rows'] == 50


# 8, 9: error text is bounded and never crashes the run
class Ugly(Exception):
    def __str__(self):
        raise ValueError('boom in __str__')


class Raising(LLMAdapter):
    name = 'raising'

    def view(self, **kw):
        raise Ugly()

    def merge(self, **kw):
        return {}


def test_error_text_is_bounded_and_safe():
    res = Council('x', Raising(), phases=PH, retries=0).run()
    assert res.rounds[0].views[0].error == 'Ugly: (message not printable)'
    m = dict(GOOD_MERGE, **{'x' * 2_000_00: 1})
    res = Council('x', ScriptedAdapter({'merges': {'p|1': [m]}}), phases=PH, retries=0).run()
    assert len(res.rounds[0].juror.error) <= 500


def test_secrets_are_redacted_from_errors():
    class Leaky(StubAdapter):
        def view(self, **kw):
            raise RuntimeError('HTTP 401 https://api.example/v1?api_key=abc123 Authorization: Bearer sk-0123456789')
    res = Council('x', Leaky(), phases=PH, retries=0).run()
    assert 'abc123' not in res.rounds[0].views[0].error and 'sk-0123456789' not in res.rounds[0].views[0].error


# 10: no line spoofing in audit output
def test_newlines_in_examples_are_escaped(tmp_path):
    doc = json.loads(json.dumps(FIX['council'][1]['input']))
    doc['rounds'][0]['step_id'] = 'frame\nfake.json: BUILD-READY (council)'
    p = tmp_path / 'x.json'
    p.write_text(json.dumps(doc))
    out = cli('audit', str(p)).stdout
    assert not any(line.startswith('fake.json') for line in out.splitlines())


# 11: a source the run did not provide is flagged even when the run provides none
def test_made_up_sources_are_flagged():
    assert gates.flag_unsourced({'sources': ['made-up'], 'contribution': '40%'}, []) == [
        {'flag': 'unknown_source', 'source': 'made-up', 'stakeholder': ''}]


# 12: imported decisions of a run that decided nothing are not shown as decisions
def test_import_drops_decisions_without_a_full_merge():
    doc = json.loads(json.dumps(FIX['import'][1]['input']))
    doc['decisions'] = [{'topic': 't', 'decision': 'Juror synthesis failed'}]
    assert from_smartsignal(doc).locked_decisions == []


# 13: duplicate phases are refused
def test_duplicate_phases_are_refused():
    with pytest.raises(ValueError):
        Council('x', StubAdapter(), phases=[('build', 'B', 'q'), ('build', 'B', 'q')])


# 14: a refused export writes nothing
def test_export_is_all_or_nothing(tmp_path):
    out = tmp_path / 'o'
    out.mkdir()
    (out / 'spec.md').write_text('mine')
    r = cli('run', '--idea', 'x', '--out', str(out))
    assert r.returncode == 1 and sorted(p.name for p in out.iterdir()) == ['spec.md']


# 15: --adapter refuses a non-adapter before calling it
def test_adapter_must_be_an_adapter_class(tmp_path):
    marker = tmp_path / 'ran'
    r = cli('run', '--idea', 'x', '--adapter', 'pathlib:Path', '--adapter-arg', f'x={marker}', '--out', str(tmp_path / 'o'))
    assert r.returncode == 1 and 'not an LLMAdapter class' in r.stderr


# private robustness: methodology guidance only for the matching phase
def test_methodology_only_for_matching_titles():
    res = Council('x', StubAdapter(), phases=[('concept', 'Sources: where the data comes from', 'q?'),
                                              ('frame', 'Frame', 'q?')]).run()
    assert [r['meta']['phase'] for r in to_trainset(res)] == ['frame']


def test_mermaid_labels_are_quoted_and_short():
    m = dict(GOOD_MERGE, decision='"] --> evil[("' + 'x' * 200)
    res = Council('x', ScriptedAdapter({'merges': {'p|1': [m]}}), phases=PH).run()
    mm = to_mermaid(res)
    assert '"] -->' not in mm and all(len(line) < 140 for line in mm.splitlines())
    to_stories(res, 'K')


# second robustness pass
def test_council_inputs_with_surrogates_are_refused():
    for kw in ({'idea': 'caf\udce9 idea'}, {'idea': 'x', 'roster': ['a\ud800']}, {'idea': 'x', 'sources': ['\udfff']},
               {'idea': 'x', 'phases': [('p', 'T\ud800', 'q')]}):
        with pytest.raises(ValueError):
            Council(adapter=StubAdapter(), **kw)


def test_controls_and_bidi_never_reach_documents():
    m = dict(GOOD_MERGE, merge_summary='ok \x1b[2J ‮desrever', decision='\x1b]8;;http://a.invalid\x07click',
             next_steps=['Implement \x1b[31mthe parser'])
    res = Council('idea', ScriptedAdapter({'merges': {'p|1': [m]}}), phases=PH).run()
    for doc in (to_spec_md(res), to_mermaid(res)):
        assert '\x1b' not in doc and '‮' not in doc and '\x07' not in doc


def test_empty_lines_are_bounded(tmp_path, monkeypatch):
    import sf_smartsavant.audit as A
    monkeypatch.setattr(A, 'MAX_LINES', 1000)
    z = tmp_path / 'empty.zip'
    with zipfile.ZipFile(z, 'w', zipfile.ZIP_DEFLATED) as zf:
        zf.writestr('m.jsonl', '\n' * 100000 + '{"output": "late"}\n')
    assert audit_path(str(z))['summary']['rows'] == 0


def test_stub_tags_are_not_figures():
    res = Council('x', StubAdapter()).run()
    assert res.flags == []


def test_a_bare_list_of_rounds_is_audited(tmp_path):
    p = tmp_path / 'rounds.json'
    p.write_text(json.dumps(FIX['council'][1]['input']['rounds']))
    assert audit_path(str(p))['verdict'] == 'not build-ready'


def test_surrogate_in_an_unknown_key_keeps_records_encodable():
    res = Council('idea', ScriptedAdapter({'views': {'p|1|Developer': [{'\ud800': 1}]}}), roster=['Developer'],
                  phases=PH, retries=0).run()
    assert '�' in res.rounds[0].views[0].error
    run_digest(res) and to_spec_md(res) and to_stories(res, 'K')
