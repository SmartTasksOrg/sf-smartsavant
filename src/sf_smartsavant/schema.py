"""Validation of what an adapter returns, and of council files (spec/schema/*.json).

The engine never trusts an adapter: every view and every merge is checked against the contract before
it is used, and a value that fails is retried and then recorded as ``failed``. That is what keeps a run
schema-locked whatever model sits behind the adapter (and whatever it was asked to do by text injected
into the idea or the sources).

    errors = check_view(obj)      # [] when valid; otherwise human-readable reasons, first problem first
    errors = check_merge(obj)
    errors = check_council(obj)   # a council.json written by this package

The rules (also in spec/SEMANTICS.md, "Adapter contract"):
- every string at most 8000 characters (``spec_delta`` 20000), no NUL characters;
- every list at most 20 items, every item a non-empty string of at most 2000 characters;
- source ids match ``[A-Za-z0-9._:/#@-]{1,200}``;
- no keys other than the listed ones (an adapter that adds keys is wrong about the contract);
- no unpaired UTF-16 surrogates (U+D800-U+DFFF) in any string (they cannot be written as UTF-8);
- ``missing`` may be a string (one item) or a list; ``devils_advocate`` and ``merge_summary`` must not be
  empty; ``loop_recommended`` is a real boolean.
"""
from __future__ import annotations

import re

MAX_STR = 8000
MAX_DELTA = 20000
MAX_ITEMS = 20
MAX_ITEM = 2000
SOURCE_ID = re.compile(r'[A-Za-z0-9._:/#@-]{1,200}\Z')
W5H1_KEYS = ('who', 'what', 'where', 'when', 'why', 'how')
VIEW_KEYS = {'w5h1', 'done_so_far', 'missing', 'devils_advocate', 'contribution', 'sources'}
MERGE_KEYS = {'merge_summary', 'decision', 'resolved', 'open_issues', 'next_steps', 'reversals', 'spec_delta',
              'loop_recommended'}
MERGE_REQUIRED = MERGE_KEYS - {'reversals'}
_WS = re.compile(r'[ \t\n\r\f\v]*\Z')
_SURROGATE = re.compile('[\ud800-\udfff]')
MAX_DEPTH = 32
MAX_TOTAL = 1_000_000          # code points of all strings in one answer, keys included


class NotPlainData(ValueError):
    pass


def plain(obj):
    """A copy of ``obj`` made only of exact dict / list / str / int / bool / None (and float), checked
    before anything else, so that what is validated is exactly what is used. Subclasses (a dict that lies
    in ``__getitem__``, a str with a fake ``__len__``), other types, keys that are not strings, nesting
    deeper than 32 and more than 1,000,000 code points of text raise NotPlainData."""
    budget = [MAX_TOTAL]

    def go(x, depth):
        if depth > MAX_DEPTH:
            raise NotPlainData('nested deeper than 32 levels')
        t = type(x)
        if x is None or t is bool or t is int or t is float:
            return x
        if t is str:
            budget[0] -= len(x)
            if budget[0] < 0:
                raise NotPlainData('more than 1,000,000 characters of text')
            return str.__str__(x)
        if t is list:
            return [go(v, depth + 1) for v in list.__iter__(x)]
        if t is dict:
            out = {}
            for k, v in dict.items(x):
                if type(k) is not str:
                    raise NotPlainData('object keys must be strings')
                budget[0] -= len(k)
                out[str.__str__(k)] = go(v, depth + 1)
            if budget[0] < 0:
                raise NotPlainData('more than 1,000,000 characters of text')
            return out
        raise NotPlainData(f'{t.__name__} is not JSON data')
    return go(obj, 0)


def _key(k) -> str:
    """An unknown key as shown in an error: control characters (U+0000-U+001F, U+007F-U+009F) dropped,
    unpaired surrogates as U+FFFD, then at most 40 code points."""
    k = ''.join(c for c in str(k) if not (ord(c) < 32 or 127 <= ord(c) < 160))
    k = _SURROGATE.sub('\ufffd', k)                     # error text is recorded: it must stay encodable
    return k[:40] + ('...' if len(k) > 40 else '')


def _blank(s: str) -> bool:
    """Only [ \t\n\r\f\v] (or nothing): other Unicode spaces count as content (SEMANTICS section 2)."""
    return bool(_WS.match(s))


def _str(errs, where, v, limit=MAX_STR, nonempty=False):
    if not isinstance(v, str):
        errs.append(f'{where} must be a string')
    elif len(v) > limit:
        errs.append(f'{where} is longer than {limit} characters')
    elif '\x00' in v:
        errs.append(f'{where} contains a NUL character')
    elif _SURROGATE.search(v):
        errs.append(f'{where} contains an unpaired surrogate')
    elif nonempty and _blank(v):
        errs.append(f'{where} must not be empty')


def _list(errs, where, v, pattern=None):
    if not isinstance(v, list):
        errs.append(f'{where} must be a list of strings')
        return
    if len(v) > MAX_ITEMS:
        errs.append(f'{where} has more than {MAX_ITEMS} items')
    for i, x in enumerate(v[:MAX_ITEMS + 1]):
        if not isinstance(x, str) or _blank(x):
            errs.append(f'{where}[{i}] must be a non-empty string')
        elif len(x) > MAX_ITEM:
            errs.append(f'{where}[{i}] is longer than {MAX_ITEM} characters')
        elif pattern is not None and not pattern.match(x):
            errs.append(f'{where}[{i}] is not a valid source id')
        elif '\x00' in x:
            errs.append(f'{where}[{i}] contains a NUL character')
        elif _SURROGATE.search(x):
            errs.append(f'{where}[{i}] contains an unpaired surrogate')


def check_view(v) -> list:
    errs: list = []
    if not isinstance(v, dict):
        return ['a view must be a JSON object']
    extra = sorted(set(v) - VIEW_KEYS, key=str)
    if extra:
        errs.append(f'unknown keys ({len(extra)}): ' + ', '.join(_key(k) for k in extra[:5]))
    missing = sorted(VIEW_KEYS - set(v))
    if missing:
        errs.append('missing keys: ' + ', '.join(missing))
        return errs
    w = v['w5h1']
    if not isinstance(w, dict) or set(w) != set(W5H1_KEYS):
        errs.append('w5h1 must be an object with exactly who, what, where, when, why, how')
    else:
        for k in W5H1_KEYS:
            _str(errs, f'w5h1.{k}', w[k])
    _str(errs, 'done_so_far', v['done_so_far'])
    if isinstance(v['missing'], str):
        _str(errs, 'missing', v['missing'])
    else:
        _list(errs, 'missing', v['missing'])
    _str(errs, 'devils_advocate', v['devils_advocate'], nonempty=True)
    _str(errs, 'contribution', v['contribution'])
    _list(errs, 'sources', v['sources'], SOURCE_ID)
    return errs


def check_merge(m) -> list:
    errs: list = []
    if not isinstance(m, dict):
        return ['a merge must be a JSON object']
    extra = sorted(set(m) - MERGE_KEYS, key=str)
    if extra:
        errs.append(f'unknown keys ({len(extra)}): ' + ', '.join(_key(k) for k in extra[:5]))
    missing = sorted(MERGE_REQUIRED - set(m))
    if missing:
        errs.append('missing keys: ' + ', '.join(missing))
        return errs
    _str(errs, 'merge_summary', m['merge_summary'], nonempty=True)
    _str(errs, 'decision', m['decision'])
    for k in ('resolved', 'open_issues', 'next_steps'):
        _list(errs, k, m[k])
    if 'reversals' in m:
        _list(errs, 'reversals', m['reversals'])
    _str(errs, 'spec_delta', m['spec_delta'], limit=MAX_DELTA)
    if not isinstance(m['loop_recommended'], bool):
        errs.append('loop_recommended must be true or false')
    return errs


def normalize_view(v: dict) -> dict:
    """A valid view with ``missing`` always a list."""
    out = dict(v)
    if isinstance(out['missing'], str):
        out['missing'] = [] if _blank(out['missing']) else [out['missing']]
    return out


# ---------------------------------------------------------------- council files
_STATUS_V = ('ok', 'failed', 'offline')
_STATUS_J = ('full', 'thin', 'recovered', 'failed', 'offline', 'no_views')
_TOP = {'schema', 'idea', 'adapter', 'synthetic', 'stakeholders', 'sources', 'rounds', 'decision_tree',
        'locked_decisions', 'flags'}
_VIEW_REC = {'stakeholder': 's', 'role': 's', 'status': 's', 'w5h1': 'w', 'done_so_far': 's', 'missing': 'l',
             'devils_advocate': 's', 'contribution': 's', 'sources': 'l', 'error': 's'}
_JUROR_REC = {'synthesis_status': 's', 'merge_summary': 's', 'decision': 's', 'resolved': 'l', 'open_issues': 'l',
              'next_steps': 'l', 'reversals': 'l', 'spec_delta': 's', 'loop_recommended': 'b', 'error': 's'}
_ROUND_REC = {'step_id': 's', 'step_title': 's', 'iteration': 'i', 'question': 's', 'views': 'L', 'juror': 'J'}
_NODE_REC = {'step_id': 's', 'iteration': 'i', 'branches': 'l', 'decision': 's', 'next_steps': 'l'}


def _typed(errs, where, d, spec) -> None:
    if type(d) is not dict or set(d) != set(spec):
        errs.append(f'{where} must have exactly the keys ' + ', '.join(spec))
        return
    for k, t in spec.items():
        v = d[k]
        ok = {'s': type(v) is str and not _SURROGATE.search(v),
              'i': type(v) is int and v >= 1,
              'b': type(v) is bool,
              'l': type(v) is list and all(type(x) is str and not _SURROGATE.search(x) for x in v),
              'w': type(v) is dict and set(v) == set(W5H1_KEYS) and all(type(x) is str and not _SURROGATE.search(x) for x in v.values()),
              'L': type(v) is list, 'J': type(v) is dict}[t]
        if not ok:
            errs.append(f'{where}.{k} has the wrong type')


def check_council(d) -> list:
    """A council.json written by this package: exact keys and JSON types everywhere, known statuses, no
    unpaired surrogates. A file that passes can be exported and audited without errors."""
    errs: list = []
    if type(d) is not dict:
        return ['a council file must be a JSON object']
    if d.get('schema') != 'smartsavant.council/1':
        return ['schema must be "smartsavant.council/1"']
    if set(d) != _TOP:
        return ['top-level keys must be exactly ' + ', '.join(sorted(_TOP))]
    for k in ('idea', 'adapter'):
        if type(d[k]) is not str or _SURROGATE.search(d[k]):
            errs.append(f'{k} must be a string')
    if type(d['synthetic']) is not bool:
        errs.append('synthetic must be true or false')
    for k in ('stakeholders', 'sources', 'locked_decisions'):
        if type(d[k]) is not list or not all(type(x) is str and not _SURROGATE.search(x) for x in d[k]):
            errs.append(f'{k} must be a list of strings')
    if type(d['rounds']) is not list or type(d['decision_tree']) is not list or type(d['flags']) is not list:
        return errs + ['rounds, decision_tree and flags must be lists']
    if len(d['decision_tree']) != len(d['rounds']):
        errs.append('decision_tree must have one node per round')
    for i, r in enumerate(d['rounds']):
        w = f'rounds[{i}]'
        _typed(errs, w, r, _ROUND_REC)
        if type(r) is not dict or type(r.get('views')) is not list or type(r.get('juror')) is not dict:
            continue
        for j, v in enumerate(r['views']):
            _typed(errs, f'{w}.views[{j}]', v, _VIEW_REC)
            if type(v) is dict and v.get('status') not in _STATUS_V:
                errs.append(f'{w}.views[{j}].status must be one of {", ".join(_STATUS_V)}')
        _typed(errs, f'{w}.juror', r['juror'], _JUROR_REC)
        if r['juror'].get('synthesis_status') not in _STATUS_J:
            errs.append(f'{w}.juror.synthesis_status must be one of {", ".join(_STATUS_J)}')
    for i, n in enumerate(d['decision_tree']):
        _typed(errs, f'decision_tree[{i}]', n, _NODE_REC)
    for i, f in enumerate(d['flags']):
        if type(f) is not dict or not all(type(k) is str for k in f) or not all(
                type(x) in (str, int) and not (type(x) is str and _SURROGATE.search(x)) for x in f.values()):
            errs.append(f'flags[{i}] must be an object of strings and integers')
    return errs[:50]
