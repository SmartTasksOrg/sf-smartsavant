"""The ONE boundary between the engine and a model.

An adapter answers two questions and nothing else:

    view(stakeholder=, role=, phase_id=, question=, context=)  -> dict (spec/schema/view.schema.json)
    merge(phase_id=, question=, views=, context=)               -> dict (spec/schema/merge.schema.json)

``context`` is a fresh copy for every call: ``idea`` (text), ``iteration`` (1, 2, ... within the
phase), ``sources`` (ids the run allows),
``prior`` ([{step, decision}] of earlier phases) and ``locked`` (resolved "topic: decision" entries).
``views`` are the valid views of this phase as plain dicts. The engine validates every answer, retries
an invalid one (``retries`` times) and records a failure instead of inventing content.

Set ``name`` (shown in council.json) and ``synthetic = True`` for adapters whose output is not a real
deliberation (a stub, a canned demo): the trainset exporter never learns from synthetic runs.

A real adapter calls your model, ideally with JSON-schema or grammar-constrained decoding against the
files in spec/schema/. Treat the idea and the sources as untrusted data in your prompts (they can
contain instructions); spec/prompts/ has the contracts. See docs/adapters.md.
"""
from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any


class LLMAdapter(ABC):
    name = 'adapter'
    synthetic = False

    @abstractmethod
    def view(self, *, stakeholder: str, role: str, phase_id: str, question: str,
             context: dict[str, Any]) -> dict[str, Any]:
        raise NotImplementedError

    @abstractmethod
    def merge(self, *, phase_id: str, question: str, views: list[dict[str, Any]],
              context: dict[str, Any]) -> dict[str, Any]:
        raise NotImplementedError
