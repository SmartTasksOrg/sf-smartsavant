"""Replays the answers of a recorded run (a council.json written by this package), in call order.

Uses: re-running the exporters and gates on a recorded run, reproducing a bug, and testing a port
against the exact answers a real model gave. Views and merges whose status was not ``ok``/``full``-like
are replayed as failures (the adapter raises), so the replayed run has the same health as the original.
"""
from __future__ import annotations

import json

from .base import LLMAdapter


class ReplayError(RuntimeError):
    pass


class ReplayAdapter(LLMAdapter):
    name = 'replay'

    def __init__(self, recorded: dict | str):
        if isinstance(recorded, str):
            with open(recorded, encoding='utf-8') as f:
                recorded = json.load(f)
        self.synthetic = bool(recorded.get('synthetic', False))
        self._views: dict = {}
        self._merges: dict = {}
        for r in recorded.get('rounds', []):
            key = (r['step_id'], r['iteration'])
            for v in r.get('views', []):
                self._views.setdefault(key + (v['stakeholder'],), v)
            self._merges[key] = r.get('juror', {})

    def view(self, *, stakeholder, role, phase_id, question, context):
        v = self._views.get((phase_id, context.get('iteration', 1), stakeholder))
        if v is None or v.get('status') != 'ok':
            raise ReplayError(f'no recorded valid view for {stakeholder} at {phase_id}')
        return {'w5h1': dict(v['w5h1']), 'done_so_far': v['done_so_far'], 'missing': list(v['missing']),
                'devils_advocate': v['devils_advocate'], 'contribution': v['contribution'],
                'sources': list(v['sources'])}

    def merge(self, *, phase_id, question, views, context):
        m = self._merges.get((phase_id, context.get('iteration', 1)))
        if m is None or m.get('synthesis_status') not in ('full', 'thin'):
            raise ReplayError(f'no recorded valid merge for {phase_id}')
        return {k: m[k] for k in ('merge_summary', 'decision', 'resolved', 'open_issues', 'next_steps',
                                  'reversals', 'spec_delta', 'loop_recommended')}
