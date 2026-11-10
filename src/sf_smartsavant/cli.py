"""sf-smartsavant command line. ``sf-smartsavant --help`` lists the commands.

    sf-smartsavant demo                                   # a stub council, its exports and its audit
    sf-smartsavant run --idea "..." --adapter stub --out run1
    sf-smartsavant run --idea-file idea.txt --adapter mypkg.adapters:MyAdapter --adapter-arg endpoint=http://... --out run2
    sf-smartsavant audit council.json [stories.json trainset.jsonl ...] [--json]
    sf-smartsavant import smartsignal_export.json --out council.json
    sf-smartsavant export council.json --out dir --project-key KEY
    sf-smartsavant validate council.json
    sf-smartsavant vectors [DIR]

``--adapter module:Class`` imports and runs that Python code: use only your own adapters. Secrets for an
adapter come from the environment inside the adapter, never from the command line.
"""
from __future__ import annotations

import argparse
import json
import sys

from . import __version__

_CONTROL = {c: f'\\x{c:02x}' for c in list(range(32)) + list(range(127, 160)) if c != 10}
_CONTROL.update({c: f'\\u{c:04x}' for c in list(range(0x202A, 0x202F)) + list(range(0x2066, 0x206A))})
_CONTROL.update({c: f'\\u{c:04x}' for c in range(0xD800, 0xE000)})


def _safe(text) -> str:
    """Text made safe for a terminal: control characters (escape sequences), bidirectional overrides and
    unpaired surrogates shown as escapes. Newlines between lines are kept; model text inside a line is
    escaped line by line before (audit.esc)."""
    return str(text).translate(_CONTROL)


def _load_council(path):
    from .audit import esc
    from .model import result_from_dict
    from .schema import check_council
    with open(path, encoding='utf-8') as f:
        doc = json.load(f)
    errs = check_council(doc)
    if errs:
        raise SystemExit(_safe(f'{esc(path)}: not a valid council file: ' + esc('; '.join(errs[:5]))))
    return result_from_dict(doc)


def c_run(a):
    from .audit import esc
    from .engine import Council, health, load_adapter, load_phases
    from .exporters import write_all
    if bool(a.idea) == bool(a.idea_file):
        raise SystemExit('give --idea TEXT or --idea-file FILE')
    idea = a.idea
    if a.idea_file:
        with open(a.idea_file, encoding='utf-8') as f:
            idea = f.read().strip()
    args = {}
    for kv in a.adapter_arg or []:
        if '=' not in kv:
            raise SystemExit('--adapter-arg is KEY=VALUE')
        k, v = kv.split('=', 1)
        args[k] = v
    if args and (a.adapter == 'stub' or a.adapter.startswith('replay:')):
        raise SystemExit('--adapter-arg is only for module:Class adapters')
    adapter = load_adapter(a.adapter, args)
    phases = load_phases(a.phases) if a.phases else None
    roster = [r.strip() for r in a.roster.split(',')] if a.roster else None
    sources = [s.strip() for s in a.sources.split(',')] if a.sources else None
    res = Council(idea, adapter, roster=roster, phases=phases, sources=sources, max_loops=a.max_loops,
                  retries=a.retries).run()
    write_all(res, a.out, a.project_key)
    h = health(res)
    print(_safe(f'{esc(a.out)}: {h["phases_decided"]}/{h["phases"]} phases decided; views {h["views"]}; '
                f'merges {h["merges"]}; flags {h["flags"]}{"; synthetic adapter" if h["synthetic"] else ""}'))
    print(_safe(f'next: sf-smartsavant audit {esc(a.out)}/council.json {esc(a.out)}/stories.json {esc(a.out)}/trainset.jsonl'))


def c_audit(a):
    from .audit import audit_path, render
    worst = 0
    reps = {}
    for p in a.files:
        try:
            rep = audit_path(p)
        except Exception as e:                    # a file that cannot be read is exit 2, never a traceback
            from .audit import esc
            print(_safe(f'{esc(p)}: cannot audit: {esc(type(e).__name__)}: {esc(str(e)[:300])}'), file=sys.stderr)
            worst = 2
            continue
        reps[p] = rep
        if rep['verdict'] != 'build-ready':
            worst = max(worst, 1)
        if not a.json:
            print(_safe(render(rep, p)))
    if a.json:
        print(json.dumps(reps, indent=1, ensure_ascii=True))
    sys.exit(worst)


def c_import(a):
    from .importer import from_smartsignal
    with open(a.file, encoding='utf-8') as f:
        doc = json.load(f)
    res = from_smartsignal(doc)
    from .exporters import write_new
    write_new(a.out, json.dumps(res.to_dict(), indent=1, ensure_ascii=False) + '\n')
    from .audit import esc
    from .engine import health
    print(_safe(f'{esc(a.out)}: {json.dumps(health(res), sort_keys=True)}'))


def c_export(a):
    from .exporters import write_all
    res = _load_council(a.file)
    sizes = write_all(res, a.out, a.project_key)
    from .audit import esc
    print(_safe(f'{esc(a.out)}: ' + ', '.join(f'{k} ({v} bytes)' for k, v in sizes.items())))


def c_validate(a):
    _load_council(a.file)
    from .audit import esc
    print(_safe(f'{esc(a.file)}: valid'))


def c_demo(a):
    from .demo import run
    sys.exit(run())


def c_vectors(a):
    from .conformance import run_dir
    d = a.dir
    if d is None:
        from ._bundled import path
        d = str(path('conformance'))
    total, fails = run_dir(d)
    for x in fails:
        print(x)
    print(f'python: {total - len(fails)}/{total} vectors passed' if not fails else f'python: {len(fails)} of {total} failed')
    sys.exit(1 if fails else 0)


def main(argv=None):
    ap = argparse.ArgumentParser(prog='sf-smartsavant', description='A model-agnostic design council with audits.')
    ap.add_argument('--version', action='version', version=f'sf-smartsavant {__version__}')
    sub = ap.add_subparsers(dest='cmd')

    p = sub.add_parser('run', help='run a council and write council.json, spec.md, stories.json, trainset.jsonl')
    p.add_argument('--idea')
    p.add_argument('--idea-file')
    p.add_argument('--adapter', default='stub', help='stub, replay:FILE or module:Class (default stub)')
    p.add_argument('--adapter-arg', action='append', help='KEY=VALUE passed to a module:Class adapter (repeat)')
    p.add_argument('--roster', help='comma-separated stakeholder names (default: Developer, UX/UI, Product Owner, Architecture/Infra)')
    p.add_argument('--phases', help='a phases JSON file (default: spec/phases.json)')
    p.add_argument('--sources', help='comma-separated source ids the views may cite')
    p.add_argument('--max-loops', type=int, default=1)
    p.add_argument('--retries', type=int, default=1)
    p.add_argument('--project-key', default='PROJECT')
    p.add_argument('--out', required=True, help='output folder (existing files are not overwritten)')
    p.set_defaults(fn=c_run)

    p = sub.add_parser('audit', help='is this run, these stories, this trainset fit to build or train from?')
    p.add_argument('files', nargs='+')
    p.add_argument('--json', action='store_true')
    p.set_defaults(fn=c_audit)

    p = sub.add_parser('import', help='a SmartSignal council export -> council.json')
    p.add_argument('file')
    p.add_argument('--out', required=True)
    p.set_defaults(fn=c_import)

    p = sub.add_parser('export', help='council.json -> spec.md, stories.json, trainset.jsonl, decision_tree.mmd')
    p.add_argument('file')
    p.add_argument('--out', required=True)
    p.add_argument('--project-key', default='PROJECT')
    p.set_defaults(fn=c_export)

    p = sub.add_parser('validate', help='check a council.json against the schema')
    p.add_argument('file')
    p.set_defaults(fn=c_validate)

    p = sub.add_parser('demo', help='a stub council end to end (no model, no files written)')
    p.set_defaults(fn=c_demo)

    p = sub.add_parser('vectors', help='run the shared conformance vectors')
    p.add_argument('dir', nargs='?')
    p.set_defaults(fn=c_vectors)

    a = ap.parse_args(argv)
    if not getattr(a, 'fn', None):
        ap.print_help()
        sys.exit(2)
    try:
        a.fn(a)
    except FileExistsError as e:
        raise SystemExit(_safe(f'{e.filename} exists; not overwriting'))
    except OSError as e:
        if e.filename:
            raise SystemExit(_safe(f'{e.filename}: {e.strerror}'))
        raise SystemExit(_safe(f'error: {e}'))
    except (ValueError, TypeError, KeyError, ImportError, AttributeError, RecursionError, UnicodeError) as e:
        from .audit import esc
        raise SystemExit(_safe(f'error: {esc(type(e).__name__)}: {esc(str(e)[:500])}'))


if __name__ == '__main__':
    main()
