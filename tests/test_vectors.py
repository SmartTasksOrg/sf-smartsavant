import filecmp
import json
import os
import subprocess
import sys

from sf_smartsavant._bundled import path
from sf_smartsavant.conformance import FILES, run_dir
from sf_smartsavant.engine import DEFAULT_PHASES, load_phases

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SPEC = os.path.join(ROOT, 'spec', 'conformance')


def test_all_vectors_pass():
    total, fails = run_dir(SPEC)
    assert fails == []
    assert total == 198


def test_bundled_copies_equal_spec():
    for f in FILES:
        assert filecmp.cmp(os.path.join(SPEC, f), str(path('conformance', f)), shallow=False), f
    assert filecmp.cmp(os.path.join(ROOT, 'spec', 'phases.json'), str(path('phases.json')), shallow=False)
    for n in ('view', 'merge'):
        assert filecmp.cmp(os.path.join(ROOT, 'spec', 'schema', f'{n}.schema.json'), str(path('schema', f'{n}.schema.json')), shallow=False)


def test_generator_is_in_sync():
    r = subprocess.run([sys.executable, os.path.join(ROOT, 'spec', 'tools', 'make_vectors.py'), '--check'],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stdout + r.stderr


def test_default_phases_equal_the_phases_file():
    assert load_phases() == DEFAULT_PHASES
    assert load_phases(os.path.join(ROOT, 'spec', 'phases.json')) == DEFAULT_PHASES


def test_versions_agree():
    from sf_smartsavant import __version__
    assert f'version = "{__version__}"' in open(os.path.join(ROOT, 'pyproject.toml'), encoding='utf-8').read()
    assert json.load(open(os.path.join(ROOT, 'ports', 'node', 'package.json')))['version'] == __version__
    assert json.load(open(os.path.join(ROOT, '.smart.json')))['version'] == __version__
    ports = {
        ('rust', 'Cargo.toml'): f'version = "{__version__}"',
        ('java', 'pom.xml'): f'<version>{__version__}</version>',
        ('php', 'src', 'SmartSavant.php'): f"const VERSION = '{__version__}';",
        ('ruby', 'sf-smartsavant.gemspec'): f"s.version = '{__version__}'",
        ('ruby', 'lib', 'smartsavant.rb'): f"VERSION = '{__version__}'",
    }
    for parts, needle in ports.items():
        assert needle in open(os.path.join(ROOT, 'ports', *parts), encoding='utf-8').read(), parts
