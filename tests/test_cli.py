import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIX = json.load(open(os.path.join(ROOT, 'spec', 'conformance', 'audit.json')))['cases']


def run(*args, cwd=None):
    env = dict(os.environ, PYTHONPATH=os.pathsep.join([os.path.join(ROOT, 'src'), os.path.join(ROOT, 'tests')]))
    return subprocess.run([sys.executable, '-m', 'sf_smartsavant', *args], capture_output=True, text=True, env=env, cwd=cwd)


def test_version_demo_vectors():
    assert run('--version').stdout.startswith('sf-smartsavant 0.1.0')
    r = run('demo')
    assert r.returncode == 0 and r.stdout.strip().endswith('demo OK')
    r = run('vectors')
    assert r.returncode == 0 and '198/198' in r.stdout


def test_run_export_validate_audit(tmp_path):
    out = tmp_path / 'run'
    r = run('run', '--idea', 'an idea', '--out', str(out), '--project-key', 'K')
    assert r.returncode == 0, r.stderr
    assert {p.name for p in out.iterdir()} == {'council.json', 'spec.md', 'stories.json', 'trainset.jsonl', 'decision_tree.mmd'}
    assert run('validate', str(out / 'council.json')).returncode == 0
    assert run('run', '--idea', 'x', '--out', str(out)).returncode == 1           # no overwrite
    r = run('audit', str(out / 'council.json'))
    assert r.returncode == 1 and 'AUDIT-SYNTHETIC' in r.stdout
    r = run('export', str(out / 'council.json'), '--out', str(tmp_path / 'again'))
    assert r.returncode == 0
    assert (tmp_path / 'again' / 'spec.md').read_text() == (out / 'spec.md').read_text()


def test_module_adapter_with_args(tmp_path):
    r = run('run', '--idea', 'x', '--adapter', 'sample_adapter:Echo', '--adapter-arg', 'prefix=hi', '--out', str(tmp_path / 'o'))
    assert r.returncode == 0, r.stderr
    c = json.loads((tmp_path / 'o' / 'council.json').read_text())
    assert c['adapter'] == 'echo' and c['rounds'][0]['views'][0]['contribution'].startswith('hi: ')
    assert run('run', '--idea', 'x', '--adapter', 'stub', '--adapter-arg', 'a=b', '--out', str(tmp_path / 'p')).returncode == 1
    assert run('run', '--idea', 'x', '--adapter', 'json:dumps', '--out', str(tmp_path / 'q')).returncode == 1


def test_import_and_audit_exit_codes(tmp_path):
    good, bad = tmp_path / 'good.json', tmp_path / 'bad.json'
    good.write_text(json.dumps(FIX['council'][0]['input']))
    bad.write_text(json.dumps(FIX['council'][1]['input']))
    assert run('audit', str(good)).returncode == 0
    r = run('audit', str(good), str(bad), '--json')
    assert r.returncode == 1 and json.loads(r.stdout)[str(bad)]['verdict'] == 'not build-ready'
    assert run('audit', str(tmp_path / 'missing.json')).returncode == 2
    assert run('import', str(bad), '--out', str(tmp_path / 'c.json')).returncode == 0
    assert run('validate', str(tmp_path / 'c.json')).returncode == 0


def test_terminal_escapes_are_shown_escaped(tmp_path):
    doc = json.loads(json.dumps(FIX['council'][1]['input']))
    doc['rounds'][0]['juror']['merge_summary'] = '\x1b]0;owned\x07 synthesis failed'
    p = tmp_path / 'e.json'
    p.write_text(json.dumps(doc))
    r = run('audit', str(p))
    assert '\x1b' not in r.stdout and '\\x1b' in r.stdout
