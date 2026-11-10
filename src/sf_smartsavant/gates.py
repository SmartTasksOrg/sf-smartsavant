"""Quality gates: pure functions over plain dicts, so they port and test anywhere (vectors:
spec/conformance/gates.json and figures.json).

    figures(text)                    numbers that state a quantity (with a unit, currency, % or magnitude)
    flag_unsourced(view, allowed)    figures in a view without a source, and source ids nobody provided
    is_generic(step)                 a next step that names nothing ("Advance the build with the decided approach")
    is_fallback(text)                text a tool wrote when the model failed ("Juror synthesis failed ...")
    has_placeholder_code(text)       "TODO" stubs and `return {'ok': True}` bodies presented as code
    synthesis_ok(merge)              a merge that genuinely decided something
    compliance_block(text, rules)    reasons to block a release (a keyword stub: plug a real policy in)

These gates find the failure modes seen in real council exports: offline templates presented as
stakeholder views, fallback text presented as juror synthesis, generic next steps turned into user
stories, and figures without a source. They cannot judge whether a decision is good.
"""
from __future__ import annotations

import re

# a figure: optional currency, a number, and a unit / magnitude / percent; or a number with a decimal point
# or thousands separators. Plain small integers ("2-5 next steps", "step 3") are not figures.
_UNIT = (r'%|percent|ms|µs|us|ns|s|sec|secs|seconds?|min|mins|minutes?|h|hrs?|hours?|days?|weeks?|months?|years?|'
         r'x|k|K|m|M|b|B|bn|mn|GB|MB|KB|TB|GiB|MiB|KiB|TiB|kpps|pps|rps|qps|tps|req/s|tokens?|users?|agents?|'
         r'USD|EUR|GBP')
_FIG = re.compile(
    r'(?<![\w.])'
    r'(?:(?P<cur>[£$€])\s?)?'
    r'(?P<num>\d{1,3}(?:,\d{3})+(?:\.\d+)?|\d+\.\d+|\d+)'
    r'(?:\s?(?P<unit>' + _UNIT + r'))?'
    r'(?![\w])',
    re.ASCII,
)


_WS = re.compile(r'[ \t\n\r\f\v]+')


def squash(text: str) -> str:
    """Runs of ASCII whitespace (space, tab, LF, CR, FF, VT) to one space, trimmed."""
    return ' '.join(w for w in _WS.split(text or '') if w)


def blank(text) -> bool:
    """True for a non-string or a string made only of [ \t\n\r\f\v] (the contract's whitespace)."""
    return not isinstance(text, str) or squash(text) == ''


def figures(text: str) -> list:
    """Figures in order of appearance, as written (e.g. ``['$5,272M', '29.8%', '<5 ms' -> '5 ms']``)."""
    out = []
    for m in _FIG.finditer(text or ''):
        num = m.group('num')
        if m.group('cur') or m.group('unit') or ',' in num or '.' in num:
            out.append(m.group(0).strip())
    return out


def _view_text(view: dict) -> str:
    missing = view.get('missing', '')
    if isinstance(missing, list):
        missing = ' '.join(str(x) for x in missing)
    return ' '.join(str(x) for x in (view.get('contribution', ''), view.get('devils_advocate', ''),
                                      view.get('done_so_far', ''), missing))


def flag_unsourced(view: dict, allowed_sources: list) -> list:
    """Flags for one view, in order: every figure when the view cites no source
    (``unsourced_figures``), then every cited source id that is not among the run's ``allowed_sources``
    (``unknown_source``; when the run allows no sources, every cited id is unknown). Flagged content is
    kept and marked, never dropped. Sources given to the run do not excuse a view that cites none."""
    out = []
    who = view.get('stakeholder', '')
    cited = [s for s in view.get('sources', []) or [] if isinstance(s, str)]
    if not cited:
        for f in figures(_view_text(view)):
            out.append({'flag': 'unsourced_figures', 'figure': f, 'stakeholder': who})
    allowed = set(allowed_sources or [])
    for s in cited:
        if s not in allowed:
            out.append({'flag': 'unknown_source', 'source': s, 'stakeholder': who})
    return out


_GENERIC = [
    re.compile(r'^\s*advance the [\w/ -]+ with the decided approach[.!?]*\s*$', re.I | re.ASCII),
    re.compile(r'^\s*[A-Za-z]+\s+[XYZ][.!?]*\s*$', re.ASCII),
    re.compile(r'^\s*(next steps?\s*:?\s*)?(tbd|todo|n/?a|none|proceed|continue|next steps?)?[.!?]*\s*$',
               re.I | re.ASCII),
]


def is_generic(step: str) -> bool:
    """A next step or story title that names no real component or decision (fallback text included)."""
    return blank(step) or any(p.match(step) for p in _GENERIC) or is_fallback(step)


_FALLBACK = re.compile(r'synthesis failed|(falling|fell|fall) back to the raw panel|returned unparseable output|'
                       r'unparseable output|offline fallback|\(offline\)|fallback template', re.I | re.ASCII)


def is_fallback(text: str) -> bool:
    return bool(_FALLBACK.search(text or ''))


_PLACEHOLDER = re.compile(r"#\s*TODO\b|//\s*TODO\b|return\s*\{\s*['\"]ok['\"]\s*:\s*True\s*\}|"
                          r"raise NotImplementedError|\bpass\s*#\s*TODO", re.I | re.ASCII)


def has_placeholder_code(text: str) -> bool:
    return bool(_PLACEHOLDER.search(text or ''))


def synthesis_ok(merge: dict) -> bool:
    """A merge counts as genuine synthesis when its status is ``full`` (or absent, for a raw adapter
    answer); it has a decision and a summary; no summary, decision, resolved entry or next step is
    fallback text; it has at least one resolved entry; and at least one next step is not generic."""
    if merge.get('synthesis_status', 'full') != 'full':
        return False
    summary, decision = merge.get('merge_summary', ''), merge.get('decision', '')
    if blank(summary) or blank(decision):
        return False
    resolved = merge.get('resolved') or []
    steps = merge.get('next_steps') or []
    if any(is_fallback(t) for t in [summary, decision, *resolved, *steps]):
        return False
    return bool(resolved) and any(not is_generic(s) for s in steps)


# compliance: rules are (name, [group, group, ...]); a rule fires when the text contains at least one
# word of EVERY group. A private build replaces or extends these with a real policy model.
DEFAULT_RULES = [
    ('possible trading guidance on non-public information',
     [['non-public', 'nonpublic', 'mnpi', 'insider information'], ['buy', 'sell', 'trade', 'short', 'position']]),
    ('possible credential or secret in the output',
     [['-----begin', 'aws_secret_access_key', 'api_key=', 'password=', 'ghp_', 'github' + '_pat_', 'pypi-ag']]),
]


def compliance_block(text: str, rules=None) -> list:
    """Reasons to BLOCK a release of this text (empty list: no rule fired). Keyword matching only."""
    low = (text or '').lower()
    out = []
    for name, groups in (DEFAULT_RULES if rules is None else rules):
        if all(any(w in low for w in g) for g in groups):
            out.append(name)
    return out


_SECRET = re.compile(r'(?i)\b(api[_-]?key|access[_-]?token|token|secret|password|passwd|signature|sig|key)'
                     r'(\s*[=:]\s*)([^\s&;,"\']+)', re.ASCII)
_BEARER = re.compile(r'(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=-]{8,}', re.ASCII)


def redact(text: str) -> str:
    """Error text made safe to record: ``key=VALUE``-style secrets and bearer tokens replaced by ``***``."""
    return _BEARER.sub(lambda m: m.group(1) + ' ***', _SECRET.sub(lambda m: m.group(1) + m.group(2) + '***', text or ''))
