"""Run the shared vectors (spec/conformance/*.json) against this implementation.

    sf-smartsavant vectors [DIR]       # DIR defaults to the copy shipped in the wheel

Every port has a runner that does the same; spec/PORTING.md describes the files. One check per case,
and for runs.json one check per expected field.
"""
from __future__ import annotations

import hashlib
import json
import os

from . import canonical, gates
from .adapters import ScriptedAdapter
from .audit import audit_result, audit_stories, audit_trainset
from .engine import Council, health
from .exporters import run_digest, to_mermaid, to_spec_md, to_stories, to_trainset
from .importer import from_smartsignal
from .schema import check_merge, check_view

FILES = ('canonical.json', 'figures.json', 'gates.json', 'schema.json', 'runs.json', 'audit.json')


def _load(d, name):
    with open(os.path.join(d, name), encoding='utf-8') as f:
        return json.load(f)


def run_dir(d: str):
    total, fails = 0, []

    def check(name, got, want):
        nonlocal total
        total += 1
        if got != want:
            fails.append(f'{name}: got {json.dumps(got, sort_keys=True, ensure_ascii=False)[:300]}, '
                         f'want {json.dumps(want, sort_keys=True, ensure_ascii=False)[:300]}')

    for c in _load(d, 'canonical.json')['cases']:
        t = canonical.dumps(c['input'])
        check('canonical/' + c['name'], [t, hashlib.sha256(t.encode()).hexdigest()], [c['canonical'], c['sha256']])
    for i, c in enumerate(_load(d, 'figures.json')['cases']):
        check(f'figures/{i}', gates.figures(c['text']), c['figures'])
    g = _load(d, 'gates.json')['cases']
    for c in g['flag_unsourced']:
        check('flag_unsourced/' + c['name'], gates.flag_unsourced(c['view'], c['allowed']), c['flags'])
    for fn in ('is_generic', 'is_fallback', 'has_placeholder_code', 'compliance_block', 'squash'):
        for i, c in enumerate(g[fn]):
            check(f'{fn}/{i}', getattr(gates, fn)(c['text']), c['result'])
    for c in g['synthesis_ok']:
        check('synthesis_ok/' + c['name'], gates.synthesis_ok(c['merge']), c['result'])
    s = _load(d, 'schema.json')['cases']
    for c in s['view']:
        check('schema/view/' + c['name'], check_view(c['input']), c['errors'])
    for c in s['merge']:
        check('schema/merge/' + c['name'], check_merge(c['input']), c['errors'])
    for c in _load(d, 'runs.json')['cases']:
        ph = [(p['step_id'], p['title'], p['question']) for p in c['phases']] if c['phases'] else None
        res = Council(c['idea'], ScriptedAdapter(c['script']), roster=c['roster'], phases=ph, sources=c['sources'],
                      max_loops=c['max_loops'], retries=c['retries']).run()
        k = c['project_key']
        got = {'council': res.to_dict(), 'digest': run_digest(res), 'health': health(res),
               'stories': to_stories(res, k), 'spec_md': to_spec_md(res), 'mermaid': to_mermaid(res),
               'trainset': to_trainset(res), 'trainset_all': to_trainset(res, allow_synthetic=True),
               'audit': audit_result(res)}
        for field in c['expect']:
            check(f'runs/{c["name"]}/{field}', got[field], c['expect'][field])
    a = _load(d, 'audit.json')['cases']
    for c in a['import']:
        check('import/' + c['name'], from_smartsignal(c['input']).to_dict(), c['council'])
    for c in a['council']:
        check('audit/council/' + c['name'], audit_result(from_smartsignal(c['input'])), c['report'])
    for c in a['stories']:
        check('audit/stories/' + c['name'], audit_stories(c['input']), c['report'])
    for c in a['trainset']:
        check('audit/trainset/' + c['name'], audit_trainset(c['input']), c['report'])
    return total, fails
