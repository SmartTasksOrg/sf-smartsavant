"""SmartSavant: a model-agnostic design council. A roster of stakeholders gives structured views on each
phase of a design sprint, a juror merges them into decisions, and the run is exported as a spec, user
stories, a decision tree and a fine-tune trainset. The engine only talks to an adapter, validates every
answer, records failures instead of inventing content, and audits runs (its own or imported ones)
before anyone builds or trains from them.
"""
from . import gates
from ._version import __version__
from .adapters import LLMAdapter, ReplayAdapter, StubAdapter
from .audit import audit_path, audit_result
from .engine import DEFAULT_PHASES, DEFAULT_ROSTER, Council, health, load_phases
from .importer import from_smartsignal
from .model import CouncilResult

__all__ = ['__version__', 'Council', 'CouncilResult', 'DEFAULT_PHASES', 'DEFAULT_ROSTER', 'LLMAdapter',
           'ReplayAdapter', 'StubAdapter', 'audit_path', 'audit_result', 'from_smartsignal', 'gates', 'health',
           'load_phases']
