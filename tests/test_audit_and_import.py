import json
import os
import zipfile

from sf_smartsavant import Council, StubAdapter
from sf_smartsavant.audit import audit_path, audit_result
from sf_smartsavant.importer import detect, from_smartsignal

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIX = json.load(open(os.path.join(ROOT, 'spec', 'conformance', 'audit.json')))['cases']


def test_offline_export_is_not_build_ready_and_good_one_is():
    good, bad = FIX['council'][0], FIX['council'][1]
    assert good['report']['verdict'] == 'build-ready'
    codes = {f['code'] for f in bad['report']['findings']}
    assert {'AUDIT-NO-SYNTHESIS', 'AUDIT-OFFLINE-VIEWS', 'AUDIT-FALLBACK-TEXT', 'AUDIT-GENERIC-STEPS',
            'AUDIT-TEMPLATE-VIEWS'} <= codes
    assert detect(good['input']) == 'smartsignal-rounds'


def test_audit_path_reads_every_kind(tmp_path):
    c = tmp_path / 'council.json'
    c.write_text(json.dumps(Council('x', StubAdapter()).run().to_dict()))
    assert audit_path(str(c))['findings'][0]['code'] == 'AUDIT-SYNTHETIC'
    ss = tmp_path / 'export.json'
    ss.write_text(json.dumps(FIX['council'][1]['input']))
    assert audit_path(str(ss))['source'] == 'smartsignal-rounds'
    st = tmp_path / 'stories.json'
    st.write_text(json.dumps(FIX['stories'][0]['input']))
    assert audit_path(str(st))['verdict'] == 'not build-ready'
    rows = FIX['trainset'][0]['input']
    j = tmp_path / 't.jsonl'
    j.write_text('\n'.join(json.dumps(r) for r in rows) + '\nnot json\n')
    rep = audit_path(str(j))
    assert rep['summary']['rows'] == 5 and rep['verdict'] == 'not build-ready'
    z = tmp_path / 't.zip'
    with zipfile.ZipFile(z, 'w') as zf:
        zf.writestr('dataset/all.jsonl', '\n'.join(json.dumps(r) for r in rows))
        zf.writestr('dataset/flagged_unsourced.jsonl', json.dumps({'output': 'ignored'}))
        zf.writestr('../evil.txt', 'not read')
    assert audit_path(str(z))['summary']['rows'] == 4


def test_import_handles_repeated_steps_and_object_contributions():
    doc = FIX['import'][1]['input']
    twice = dict(doc, rounds=doc['rounds'] + doc['rounds'])
    res = from_smartsignal(twice)
    ids = [r.step_id for r in res.rounds]
    assert ids == ['frame', 'concept', 'build', 'frame.2', 'concept.2', 'build.2']
    assert 'approach:' in res.rounds[0].views[0].contribution


def test_a_full_merge_that_fails_the_gate_is_thin():
    doc = json.loads(json.dumps(FIX['import'][0]['input']))
    doc['rounds'][0]['juror']['next_steps'] = ['Advance the frame with the decided approach']
    res = from_smartsignal(doc)
    assert res.rounds[0].juror.synthesis_status == 'thin'
    assert audit_result(res)['verdict'] == 'not build-ready'
