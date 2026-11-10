// SmartSavant council types: a mirror of spec/schema/*.json and spec/SEMANTICS.md (sf-smartsavant 0.1.1).
// The Node port (ports/node) implements these; an Electron or Node app supplies its own LLMAdapter.

export interface W5H1 { who: string; what: string; where: string; when: string; why: string; how: string; }

/** What an adapter's view() must return (validated; unknown keys are an error). */
export interface ViewAnswer {
  w5h1: W5H1; done_so_far: string; missing: string | string[]; devils_advocate: string;
  contribution: string; sources: string[];
}
/** What an adapter's merge() must return. */
export interface MergeAnswer {
  merge_summary: string; decision: string; resolved: string[]; open_issues: string[]; next_steps: string[];
  reversals?: string[]; spec_delta: string; loop_recommended: boolean;
}

export type ViewStatus = 'ok' | 'failed' | 'offline';
export type SynthesisStatus = 'full' | 'thin' | 'recovered' | 'failed' | 'offline' | 'no_views';

export interface StakeholderView {
  stakeholder: string; role: string; status: ViewStatus; w5h1: W5H1; done_so_far: string; missing: string[];
  devils_advocate: string; contribution: string; sources: string[]; error: string;
}
export interface JurorMerge {
  synthesis_status: SynthesisStatus; merge_summary: string; decision: string; resolved: string[];
  open_issues: string[]; next_steps: string[]; reversals: string[]; spec_delta: string;
  loop_recommended: boolean; error: string;
}
export interface CouncilRound {
  step_id: string; step_title: string; iteration: number; question: string;
  views: StakeholderView[]; juror: JurorMerge;
}
export interface DecisionNode { step_id: string; iteration: number; branches: string[]; decision: string; next_steps: string[]; }
export interface Flag { flag: 'unsourced_figures' | 'unknown_source'; figure?: string; source?: string; stakeholder: string; step_id: string; iteration: number; }
export interface CouncilResult {
  schema: 'smartsavant.council/1'; idea: string; adapter: string; synthetic: boolean; stakeholders: string[];
  sources: string[]; rounds: CouncilRound[]; decision_tree: DecisionNode[]; locked_decisions: string[]; flags: Flag[];
}

export interface AdapterContext {
  idea: string; iteration: number; sources: string[]; prior: { step: string; decision: string }[]; locked: string[];
}
export interface LLMAdapter {
  readonly name: string;
  /** true for stubs and demos: their output is never learned from or built on */
  readonly synthetic: boolean;
  view(a: { stakeholder: string; role: string; phase_id: string; question: string; context: AdapterContext }): ViewAnswer | Promise<ViewAnswer>;
  merge(a: { phase_id: string; question: string; views: StakeholderView[]; context: AdapterContext }): MergeAnswer | Promise<MergeAnswer>;
}
