"""A deterministic adapter: no model, no network. For tests, demos and ports (the vectors pin its
output). It is ``synthetic``: its "views" are templates, so nothing it produces is a real decision, and
the trainset exporter excludes it.

    tag  = first 8 hex digits of sha256(UTF-8(stakeholder + "|" + phase_id + "|" + idea)), written as
           "sv" + tag so that it never reads as a figure (e.g. "4007743b" would be "4007743 billion")
    idea_head = the first 120 Unicode code points of the idea
"""
from __future__ import annotations

import hashlib

from .base import LLMAdapter


def _tag(*parts: str) -> str:
    return hashlib.sha256('|'.join(parts).encode('utf-8')).hexdigest()[:8]


class StubAdapter(LLMAdapter):
    name = 'stub'
    synthetic = True

    def view(self, *, stakeholder, role, phase_id, question, context):
        idea = context.get('idea', '')
        tag = _tag(stakeholder, phase_id, idea)
        return {
            'w5h1': {'who': stakeholder, 'what': f'advance {phase_id}', 'where': 'the target architecture',
                     'when': 'this iteration', 'why': idea[:120], 'how': f'deterministic stub sv{tag}'},
            'done_so_far': f'{role} framed {phase_id}.',
            'missing': [f'{phase_id}: target files and modules'],
            'devils_advocate': f'Risk at {phase_id}: an unproven assumption (sv{tag}).',
            'contribution': f'{role} proposes the {phase_id} step sv{tag}.',
            'sources': [],
        }

    def merge(self, *, phase_id, question, views, context):
        resolved = [f"{phase_id} {i + 1}: {v['contribution']}" for i, v in enumerate(views)][:5]
        issues = list(dict.fromkeys(m for v in views for m in v.get('missing', [])))[:5]
        return {
            'merge_summary': f'Stub juror merged {len(views)} view(s) for {phase_id}.',
            'decision': f'{phase_id}: proceed with the stub plan',
            'resolved': resolved,
            'open_issues': issues,
            'next_steps': [f'Write the {phase_id} module for the stub plan'],
            'reversals': [],
            'spec_delta': f'[{phase_id}] stub decision recorded.',
            'loop_recommended': False,
        }
