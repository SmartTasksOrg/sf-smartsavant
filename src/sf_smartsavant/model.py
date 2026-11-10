"""The council data model (mirrors spec/schema/council.schema.json). Pure stdlib dataclasses.

Status words used throughout (spec/SEMANTICS.md, "Statuses"):
    view.status       ok | failed | offline
        ok        the adapter returned a valid view
        failed    the adapter raised or returned something invalid, also after the retries
        offline   imported from a run whose view came from an offline template, not a model
    juror.synthesis_status   full | thin | recovered | failed | offline | no_views
        full       a valid merge that decided something (see gates.synthesis_ok)
        thin       a valid merge that did not decide anything usable
        recovered  imported: the tool salvaged text after the model failed
        failed     the adapter raised or returned something invalid, also after the retries
        offline    imported: an offline template, no model
        no_views   no valid view to merge, so the juror was not called
"""
from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Any

VIEW_STATUSES = ('ok', 'failed', 'offline')
SYNTHESIS_STATUSES = ('full', 'thin', 'recovered', 'failed', 'offline', 'no_views')


@dataclass
class W5H1:
    who: str = ''
    what: str = ''
    where: str = ''
    when: str = ''
    why: str = ''
    how: str = ''


@dataclass
class StakeholderView:
    stakeholder: str
    role: str
    status: str = 'ok'
    w5h1: W5H1 = field(default_factory=W5H1)
    done_so_far: str = ''
    missing: list = field(default_factory=list)
    devils_advocate: str = ''
    contribution: str = ''
    sources: list = field(default_factory=list)
    error: str = ''


@dataclass
class JurorMerge:
    synthesis_status: str = 'full'
    merge_summary: str = ''
    decision: str = ''
    resolved: list = field(default_factory=list)
    open_issues: list = field(default_factory=list)
    next_steps: list = field(default_factory=list)
    reversals: list = field(default_factory=list)
    spec_delta: str = ''
    loop_recommended: bool = False
    error: str = ''


@dataclass
class CouncilRound:
    step_id: str
    step_title: str
    iteration: int
    question: str
    views: list = field(default_factory=list)
    juror: JurorMerge = field(default_factory=JurorMerge)


@dataclass
class DecisionNode:
    step_id: str
    iteration: int
    branches: list = field(default_factory=list)
    decision: str = ''
    next_steps: list = field(default_factory=list)


@dataclass
class CouncilResult:
    schema: str = 'smartsavant.council/1'
    idea: str = ''
    adapter: str = ''
    synthetic: bool = False
    stakeholders: list = field(default_factory=list)
    sources: list = field(default_factory=list)
    rounds: list = field(default_factory=list)
    decision_tree: list = field(default_factory=list)
    locked_decisions: list = field(default_factory=list)
    flags: list = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def result_from_dict(d: dict) -> CouncilResult:
    """The inverse of ``to_dict`` (for files written by this package; validate them first)."""
    rounds = []
    for r in d.get('rounds', []):
        views = [StakeholderView(**{**v, 'w5h1': W5H1(**v.get('w5h1', {}))}) for v in r.get('views', [])]
        rounds.append(CouncilRound(step_id=r['step_id'], step_title=r['step_title'], iteration=r['iteration'],
                                   question=r['question'], views=views, juror=JurorMerge(**r.get('juror', {}))))
    tree = [DecisionNode(**n) for n in d.get('decision_tree', [])]
    rest = {k: v for k, v in d.items() if k not in ('rounds', 'decision_tree')}
    return CouncilResult(**rest, rounds=rounds, decision_tree=tree)
