#!/usr/bin/env python3
"""Generate spec/conformance/*.json from the Python reference.

    python spec/tools/make_vectors.py            # write the files
    python spec/tools/make_vectors.py --check    # exit 1 if the files differ from what would be written
"""
from __future__ import annotations

import copy
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 'src'))

from sf_smartsavant import canonical, gates  # noqa: E402
from sf_smartsavant.adapters import ScriptedAdapter  # noqa: E402
from sf_smartsavant.audit import audit_result, audit_stories, audit_trainset  # noqa: E402
from sf_smartsavant.engine import Council, health  # noqa: E402
from sf_smartsavant.exporters import run_digest, to_mermaid, to_spec_md, to_stories, to_trainset  # noqa: E402
from sf_smartsavant.importer import from_smartsignal  # noqa: E402
from sf_smartsavant.schema import check_merge, check_view  # noqa: E402

OUT = os.path.join(ROOT, 'spec', 'conformance')
CHECK = '--check' in sys.argv
DIFF: list = []


def write(name, obj):
    text = json.dumps(obj, indent=1, ensure_ascii=True) + '\n'
    path = os.path.join(OUT, name)
    if CHECK:
        try:
            cur = open(path, encoding='utf-8').read()
        except OSError:
            cur = None
        if cur != text:
            print(f'{name}: differs from the generator')
            DIFF.append(name)
        return
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        f.write(text)


def canonical_vectors():
    inputs = [('empty object', {}), ('empty array', []), ('null', None), ('true', True), ('false', False), ('zero', 0),
              ('negative', -42), ('max safe integer', 2 ** 53 - 1), ('sorted keys', {'b': 1, 'a': 2, 'B': 3, '_': 4, 'a1': 5}),
              ('nested', {'z': [1, {'y': None, 'x': [True, False]}], 'a': {'c': 'd'}}),
              ('escapes', 'quote " backslash \\ slash / bs \b ff \f nl \n cr \r tab \t'), ('control characters', '\x00\x01\x1f\x7f'),
              ('non-ASCII as is', 'ü 漢字 🙂  '), ('empty string', '')]
    cases = []
    for name, v in inputs:
        t = canonical.dumps(v)
        cases.append({'name': name, 'input': v, 'canonical': t, 'sha256': hashlib.sha256(t.encode()).hexdigest()})
    return {'description': 'canonical(input) equals "canonical"; sha256 of its UTF-8 bytes', 'cases': cases}


FIG_TEXTS = [
    'revenue was $5,272M and 29.8% of it', 'p99 under <5 ms, CPU < 2% avg', 'return 2-5 next steps in step 3',
    'founded in 2026 with 12 users', '1,000,000 rows and 3.14 pies', '€40k budget, £3 fee, 7x faster, 500kpps',
    '30s window ±200ms jitter', 'v1.2.3 and x86_64 are not figures', 'ab12 cd 34ef', '', '10 GB, 2 TB, 64 MiB',
    'weekly retention ≥ 30%', '40% of AI developers run frame in week 1', 'time-to-first-value < 5 min',
]


def figures_vectors():
    return {'description': 'figures(text) per gates.py and SEMANTICS.md "Figures"',
            'cases': [{'text': t, 'figures': gates.figures(t)} for t in FIG_TEXTS]}


def gates_vectors():
    views = [
        ('no figures, no sources', {'stakeholder': 'Dev', 'contribution': 'ship the parser first', 'devils_advocate': 'risk', 'done_so_far': '', 'missing': [], 'sources': []}, []),
        ('figures without a source', {'stakeholder': 'PO', 'contribution': 'retention 30% and 5 min to value', 'devils_advocate': 'costs $2k', 'done_so_far': '', 'missing': ['a 3.5 s budget'], 'sources': []}, []),
        ('figures with a source', {'stakeholder': 'PO', 'contribution': 'retention 30%', 'devils_advocate': '', 'done_so_far': '', 'missing': [], 'sources': ['survey-2026']}, ['survey-2026']),
        ('unknown source', {'stakeholder': 'Arch', 'contribution': 'p99 5 ms', 'devils_advocate': '', 'done_so_far': '', 'missing': [], 'sources': ['blog-x', 'survey-2026']}, ['survey-2026']),
        ('run sources do not excuse a view', {'stakeholder': 'UX', 'contribution': '40% activation', 'devils_advocate': '', 'done_so_far': '', 'missing': 'a 2 s budget', 'sources': []}, ['survey-2026']),
        ('a cited source when the run lists none is unknown', {'stakeholder': 'UX', 'contribution': '40% activation', 'devils_advocate': '', 'done_so_far': '', 'missing': [], 'sources': ['anything']}, []),
    ]
    steps = ['\u3000', '\x1c', 'Advance the build with the decided approach', 'advance the frame with the decided approach.', 'Implement X',
             'Implement the retry policy for the ingestion queue', 'TBD', 'proceed', '', '  ', 'Define the OAuth2 token scope',
             'Design Y for the app', 'Next steps', 'Advance the build phase with the decided approach',
             'Advance the user-flow with the decided approach!', 'TBD!', 'Next steps: TBD', 'Juror fell back to the raw panel',
             'Add a fallback strategy for the queue']
    texts = ["Juror synthesis failed for 'Frame' after gpt-oss-20b returned unparseable output.",
             'Falling back to the raw panel contributions for this step.', 'The juror decided to ship the parser.',
             "def frame(signal):\n    # TODO: wire Current\n    return {'ok': True}", 'return {"ok": True}', 'a plan with no code',
             '// TODO later', 'offline fallback text', 'Juror fell back to the raw panel', 'add a fallback cache']
    merges = [
        ('full', {'merge_summary': 'Merged.', 'decision': 'ship the parser', 'resolved': ['parser: first'], 'next_steps': ['Write the parser'], 'synthesis_status': 'full'}),
        ('no status given', {'merge_summary': 'Merged.', 'decision': 'ship', 'resolved': ['a: b'], 'next_steps': ['Write the parser']}),
        ('recovered', {'merge_summary': 'Merged.', 'decision': 'ship', 'resolved': ['a: b'], 'next_steps': ['Write the parser'], 'synthesis_status': 'recovered'}),
        ('no decision', {'merge_summary': 'Merged.', 'decision': '', 'resolved': ['a: b'], 'next_steps': ['Write the parser']}),
        ('nothing resolved', {'merge_summary': 'Merged.', 'decision': 'ship', 'resolved': [], 'next_steps': ['Write the parser']}),
        ('only generic steps', {'merge_summary': 'Merged.', 'decision': 'ship', 'resolved': ['a: b'], 'next_steps': ['Advance the build with the decided approach']}),
        ('fallback summary', {'merge_summary': 'Juror synthesis failed; falling back.', 'decision': 'ship', 'resolved': ['a: b'], 'next_steps': ['Write the parser']}),
        ('one good step among generic', {'merge_summary': 'Merged.', 'decision': 'ship', 'resolved': ['a: b'], 'next_steps': ['TBD', 'Write the parser']}),
        ('fallback in resolved', {'merge_summary': 'Merged.', 'decision': 'ship', 'resolved': ['Juror synthesis failed; falling back.'], 'next_steps': ['Write the parser']}),
        ('fallback in a next step', {'merge_summary': 'Merged.', 'decision': 'ship', 'resolved': ['a: b'], 'next_steps': ['returned unparseable output', 'Write the parser']}),
    ]
    comp = ['Buy the stock before the non-public results', 'MNPI: we should short it', 'public results: buy', 'non-public roadmap only',
            'here is -----BEGIN OPENSSH KEY----- x', 'set api_key=abc', 'nothing to see']
    sq = ['  a \t b\n\nc  ', '\x1c a b ', '', 'one']
    return {'description': 'gates per SEMANTICS.md "Gates"', 'cases': {
        'flag_unsourced': [{'name': n, 'view': v, 'allowed': a, 'flags': gates.flag_unsourced(v, a)} for n, v, a in views],
        'is_generic': [{'text': s, 'result': gates.is_generic(s)} for s in steps],
        'is_fallback': [{'text': t, 'result': gates.is_fallback(t)} for t in texts],
        'has_placeholder_code': [{'text': t, 'result': gates.has_placeholder_code(t)} for t in texts],
        'synthesis_ok': [{'name': n, 'merge': m, 'result': gates.synthesis_ok(m)} for n, m in merges],
        'compliance_block': [{'text': t, 'result': gates.compliance_block(t)} for t in comp],
        'squash': [{'text': t, 'result': gates.squash(t)} for t in sq],
    }}


def good_view():
    return {'w5h1': {k: k for k in ('who', 'what', 'where', 'when', 'why', 'how')}, 'done_so_far': 'x',
            'missing': ['y'], 'devils_advocate': 'z', 'contribution': 'c', 'sources': []}


def good_merge():
    return {'merge_summary': 's', 'decision': 'd', 'resolved': ['r'], 'open_issues': [], 'next_steps': ['n'],
            'reversals': [], 'spec_delta': '', 'loop_recommended': False}


def schema_vectors():
    def mv(f):
        v = good_view()
        f(v)
        return v

    def mm(f):
        m = good_merge()
        f(m)
        return m
    views = [('valid', good_view()), ('missing as a string', mv(lambda v: v.__setitem__('missing', 'one thing'))),
             ('not an object', ['x']), ('extra key', mv(lambda v: v.__setitem__('score', 3))),
             ('missing keys', {'w5h1': good_view()['w5h1'], 'contribution': 'c'}),
             ('w5h1 incomplete', mv(lambda v: v['w5h1'].pop('how'))),
             ('w5h1 value not a string', mv(lambda v: v['w5h1'].__setitem__('who', 1))),
             ('empty devils_advocate', mv(lambda v: v.__setitem__('devils_advocate', '   '))),
             ('ideographic space is content', mv(lambda v: v.__setitem__('devils_advocate', '\u3000'))),
             ('missing as a blank string', mv(lambda v: v.__setitem__('missing', ' \t'))),
             ('contribution an object', mv(lambda v: v.__setitem__('contribution', {'a': 1}))),
             ('string too long', mv(lambda v: v.__setitem__('done_so_far', 'x' * 8001))),
             ('NUL character', mv(lambda v: v.__setitem__('contribution', 'a\x00b'))),
             ('too many items', mv(lambda v: v.__setitem__('missing', ['m'] * 21))),
             ('empty list item', mv(lambda v: v.__setitem__('missing', ['ok', '']))),
             ('item too long', mv(lambda v: v.__setitem__('missing', ['x' * 2001]))),
             ('bad source id', mv(lambda v: v.__setitem__('sources', ['ok-1', 'has space']))),
             ('sources not a list', mv(lambda v: v.__setitem__('sources', 'src'))),
             ('unpaired surrogate', mv(lambda v: v.__setitem__('contribution', 'a\ud800b'))),
             ('unpaired surrogate in a list', mv(lambda v: v.__setitem__('missing', ['\udfff']))),
             ('unknown key with an unpaired surrogate', mv(lambda v: v.__setitem__('\ud800x', 1))),
             ('many unknown keys, long and with controls', mv(lambda v: v.update({('k%d\x1b' % i) * 30: 1 for i in range(7)})))]
    merges = [('valid', good_merge()), ('reversals optional', mm(lambda m: m.pop('reversals'))),
              ('not an object', None), ('extra key', mm(lambda m: m.__setitem__('confidence', 0))),
              ('missing keys', {'merge_summary': 's'}), ('empty summary', mm(lambda m: m.__setitem__('merge_summary', ''))),
              ('loop not boolean', mm(lambda m: m.__setitem__('loop_recommended', 'false'))),
              ('spec_delta at its limit', mm(lambda m: m.__setitem__('spec_delta', 'x' * 20000))),
              ('spec_delta too long', mm(lambda m: m.__setitem__('spec_delta', 'x' * 20001))),
              ('next_steps not strings', mm(lambda m: m.__setitem__('next_steps', [1, 2])))]
    return {'description': 'check_view / check_merge give these error lists ([] when valid)', 'cases': {
        'view': [{'name': n, 'input': v, 'errors': check_view(v)} for n, v in views],
        'merge': [{'name': n, 'input': m, 'errors': check_merge(m)} for n, m in merges]}}


ROSTER3 = ['Developer', 'UX/UI', 'Product Owner']
PH3 = [{'step_id': 'frame', 'title': 'Frame', 'question': 'What is the problem?'},
       {'step_id': 'build', 'title': 'Build', 'question': 'What is the technical approach?'},
       {'step_id': 'measure', 'title': 'Measure', 'question': 'How do we know it works?'}]


def run_case(name, idea, script=None, roster=None, phases=None, sources=None, max_loops=1, retries=1, key='T'):
    ph = [(p['step_id'], p['title'], p['question']) for p in phases] if phases else None
    res = Council(idea, ScriptedAdapter(script), roster=roster, phases=ph, sources=sources, max_loops=max_loops,
                  retries=retries).run()
    return {'name': name, 'idea': idea, 'script': script or {}, 'roster': roster, 'phases': phases, 'sources': sources,
            'max_loops': max_loops, 'retries': retries, 'project_key': key,
            'expect': {'council': res.to_dict(), 'digest': run_digest(res), 'health': health(res),
                       'stories': to_stories(res, key), 'spec_md': to_spec_md(res), 'mermaid': to_mermaid(res),
                       'trainset': to_trainset(res), 'trainset_all': to_trainset(res, allow_synthetic=True),
                       'audit': audit_result(res)}}


def runs_vectors():
    gv = good_view()
    loop_merge = dict(good_merge(), merge_summary='Needs another pass.', decision='frame: not yet',
                      resolved=['scope: agents only'], next_steps=['Interview three operators'], loop_recommended=True)
    final_merge = dict(good_merge(), merge_summary='Settled.', decision='frame: operators first',
                       resolved=['users: operators', 'scope: agents only'], next_steps=['Record ten operator sessions'])
    cases = [
        run_case('default roster and phases, stub answers', 'Learn reusable automations from recorded operator sessions'),
        run_case('non-ASCII idea longer than 120 code points', 'Ünïcödé idea 🙂 ' * 12, roster=ROSTER3, phases=PH3),
        run_case('failures: invalid view, raising juror, thin merge, retry that succeeds', 'A council with a broken model',
                 roster=ROSTER3, phases=PH3, script={
                     'views': {'frame|1|UX/UI': [{'w5h1': {}}, {'raise': 'connection reset'}],
                               'build|1|Developer': [{'raise': 'timeout'}, dict(gv, contribution='use 2 workers at 5 ms p99')]},
                     'merges': {'build|1': [{'raise': 'model returned unparseable output'}],
                                'measure|1': [dict(good_merge(), merge_summary='Juror synthesis failed; falling back.',
                                                   next_steps=['Advance the measure with the decided approach'])]}}),
        run_case('all views fail in one phase', 'Nobody answers', roster=ROSTER3, phases=PH3, retries=0, script={
            'views': {f'build|1|{r}': [{'raise': 'down'}] for r in ROSTER3}}),
        run_case('a loop: the juror asks for a second pass', 'Loop until settled', roster=ROSTER3, phases=PH3[:1],
                 max_loops=3, script={'merges': {'frame|1': [loop_merge], 'frame|2': [final_merge]}}),
        run_case('sources: cited, unknown and missing', 'Sourced council', roster=ROSTER3, phases=PH3[:1],
                 sources=['survey-2026'], script={'views': {
                     'frame|1|Developer': [dict(gv, contribution='retention 30%', sources=['survey-2026'])],
                     'frame|1|UX/UI': [dict(gv, contribution='p99 5 ms', sources=['blog-x'])],
                     'frame|1|Product Owner': [dict(gv, contribution='40% activation in week 1', sources=[])]}}),
        run_case('markdown and HTML in model text are made inert', '<script>alert(1)</script> idea', roster=ROSTER3[:2],
                 phases=PH3[:1], script={'merges': {'frame|1': [dict(good_merge(), merge_summary='# Heading\n<b>bold</b> | pipe \x1b[2J \u202edesrever & co',
                                                                    decision='a|b', resolved=['x|y'], next_steps=['Write <tool>'])]}}),
    ]
    return {'description': ('Council(idea, ScriptedAdapter(script), roster, phases, sources, max_loops, retries).run(); '
                            'null roster/phases mean the defaults; every expect field must match'), 'cases': cases}


def ss_fixture(offline: bool):
    views = []
    for who in ('Developer', 'Product Owner', 'Team Lead'):
        p = {'w5h1': {'who': who, 'what': 'w', 'where': 'x', 'when': 'y', 'why': 'z', 'how': 'h'},
             'done_so_far': 'sketched', 'missing': ['a benchmark'],
             'devils_advocate': f'{who}: the frame risk is the same everywhere in this template text',
             'contribution': {'approach': f'{who} ships a thin first cut', 'code_snippet': "def f(s):\n    # TODO: wire\n    return {'ok': True}"} if offline else {'approach': f'{who} picks the parser for step', 'kpis': ['activation 40% in week 1']}}
        views.append({'stakeholder': who, 'role': who.lower(), 'source': 'offline' if offline else 'llm', 'model': '' if offline else 'model-a', 'parsed': p})
    rounds = []
    for step in ('frame', 'concept', 'build'):
        vs = copy.deepcopy(views)
        if not offline:
            for v in vs:
                v['parsed']['devils_advocate'] = f"{v['stakeholder']} at {step}: a specific risk"
                v['parsed']['contribution']['approach'] += f' {step}'
        j = ({'merge_summary': f"Juror synthesis failed for '{step}' after model-b returned unparseable output.",
              'resolved': [], 'open_issues': [], 'next_steps': [f'Advance the {step} with the decided approach'],
              'spec_delta': "# TODO\nreturn {'ok': True}", 'synthesis_status': 'recovered', 'model': 'model-b'}
             if offline else
             {'merge_summary': f'Merged {step}.', 'resolved': [f'{step} topic: decided'], 'open_issues': ['one'],
              'next_steps': [f'Build the {step} parser component'], 'spec_delta': 'text', 'synthesis_status': 'full',
              'model': 'model-b', 'loop_recommended': False})
        rounds.append({'step_id': step, 'step_title': step.title(), 'round': 0, 'views': vs, 'juror': j})
    return {'exported_at': 'fixture', 'idea': {'name': 'Fixture', 'problem': 'p', 'solution': 's', 'audience': 'a'},
            'council_build': 'fixture', 'rounds': rounds}


def audit_vectors():
    good, bad = ss_fixture(False), ss_fixture(True)
    stories_bad = {'stories': [{'title': 'Advance the frame with the decided approach'}, {'title': 'Advance the build with the decided approach'}]}
    stories_mixed = {'stories': [{'title': 'Write the parser'}, {'title': 'TBD'}]}
    rows = [{'output': 'Write the parser', 'meta': {'kind': 'expert'}},
            {'output': "Juror synthesis failed for 'x'", 'meta': {'kind': 'facilitation'}},
            {'messages': [{'role': 'user', 'content': 'q'}, {'role': 'assistant', 'content': "return {'ok': True}"}], 'meta': {'kind': 'conversation'}},
            {'output': 'fine', 'meta': 'not an object'}]
    return {'description': 'import (SmartSignal shapes) and audit; reports must match', 'cases': {
        'import': [{'name': n, 'input': d, 'council': from_smartsignal(d).to_dict()} for n, d in (('good export', good), ('offline export', bad))],
        'council': [{'name': n, 'input': d, 'report': audit_result(from_smartsignal(d))} for n, d in (('good export', good), ('offline export', bad))],
        'stories': [{'name': n, 'input': d, 'report': audit_stories(d)} for n, d in (('all generic', stories_bad), ('mixed', stories_mixed), ('empty', {'stories': []}))],
        'trainset': [{'name': 'mixed rows', 'input': rows, 'report': audit_trainset(rows)},
                     {'name': 'empty', 'input': [], 'report': audit_trainset([])}],
    }}


def main():
    os.makedirs(OUT, exist_ok=True)
    write('canonical.json', canonical_vectors())
    write('figures.json', figures_vectors())
    write('gates.json', gates_vectors())
    write('schema.json', schema_vectors())
    write('runs.json', runs_vectors())
    write('audit.json', audit_vectors())
    if CHECK:
        if DIFF:
            sys.exit(1)
        print('vectors are up to date')


if __name__ == '__main__':
    main()
