"""An adapter that answers from a script: for tests, the conformance vectors and ports.

    ScriptedAdapter({
        "views":  {"frame|1|Developer": [ANSWER, ...]},     # one entry per attempt (call), in order
        "merges": {"build|1": [{"raise": "timeout"}, ANSWER]},
    })

A key not in the script is answered by the stub (StubAdapter). An entry ``{"raise": MESSAGE}`` raises
``AdapterError(MESSAGE)``; anything else is returned as the answer (valid or not: the engine checks).
After the list for a key is used up, its last entry repeats. ``synthetic`` is true.
"""
from __future__ import annotations

import copy

from .stub import StubAdapter


class AdapterError(RuntimeError):
    pass


class ScriptedAdapter(StubAdapter):
    name = 'scripted'
    synthetic = True

    def __init__(self, script: dict | None = None):
        script = script or {}
        self.views = dict(script.get('views', {}))
        self.merges = dict(script.get('merges', {}))
        self._n: dict = {}           # calls per key; view and merge keys never collide (different shapes)

    def _next(self, table, key):
        seq = table.get(key)
        if not seq:
            return None
        i = self._n.get(key, 0)
        self._n[key] = i + 1
        ans = seq[min(i, len(seq) - 1)]
        if isinstance(ans, dict) and set(ans) == {'raise'}:
            raise AdapterError(str(ans['raise']))
        return copy.deepcopy(ans)

    def view(self, *, stakeholder, role, phase_id, question, context):
        key = f"{phase_id}|{context.get('iteration', 1)}|{stakeholder}"
        if key in self.views:
            return self._next(self.views, key)
        return super().view(stakeholder=stakeholder, role=role, phase_id=phase_id, question=question, context=context)

    def merge(self, *, phase_id, question, views, context):
        key = f"{phase_id}|{context.get('iteration', 1)}"
        if key in self.merges:
            return self._next(self.merges, key)
        return super().merge(phase_id=phase_id, question=question, views=views, context=context)
