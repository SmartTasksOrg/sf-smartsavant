//! The council engine (spec/SEMANTICS.md section 4): roster -> phases -> stakeholder views -> juror merge
//! -> decision tree. Every answer is validated and retried; one that still fails is recorded as failed
//! with an error, never filled with invented text. Deterministic for a deterministic adapter.
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::adapters::{Adapter, AdapterError, Context, MergeRequest, Prior, ViewRequest};
use crate::gates::{blank_str, flag_unsourced, redact, synthesis_ok};
use crate::json::{has_lone_surrogate, obj, replace_lone_surrogates, str_list, Object, Value};
use crate::model::{CouncilResult, CouncilRound, DecisionNode, JurorMerge, StakeholderView, W5H1};
use crate::py;
use crate::schema::{check_merge, check_view, normalize_view, plain};

pub const DEFAULT_ROSTER: [&str; 4] = ["Developer", "UX/UI", "Product Owner", "Architecture/Infra"];

/// One step of the council.
#[derive(Clone, Debug, PartialEq)]
pub struct Phase {
    pub step_id: String,
    pub title: String,
    pub question: String,
}

impl Phase {
    pub fn new(step_id: &str, title: &str, question: &str) -> Phase {
        Phase { step_id: step_id.into(), title: title.into(), question: question.into() }
    }
}

/// The phases of spec/phases.json.
pub fn default_phases() -> Vec<Phase> {
    vec![
        Phase::new("frame", "Frame", "What is the problem, who are the users, and the jobs-to-be-done?"),
        Phase::new("concept", "Concept", "What is the solution direction and the wedge?"),
        Phase::new("design", "Design", "What are the user flow, data flow, and experience?"),
        Phase::new("build", "Build", "What is the technical approach?"),
        Phase::new("integrate", "Integrate", "How does it fit the target architecture?"),
        Phase::new("measure", "Measure", "What are the KPIs and success metrics?"),
        Phase::new("spec", "Spec", "Assemble the final document."),
    ]
}

/// The phases when every field is non-blank and the step ids are unique.
pub fn check_phases(phases: &[Phase]) -> Result<Vec<Phase>, String> {
    let mut seen: Vec<&str> = vec![];
    for p in phases {
        if blank_str(&p.step_id) || blank_str(&p.title) || blank_str(&p.question) {
            return Err("each phase is (step_id, title, question), three non-blank strings".into());
        }
        if seen.contains(&p.step_id.as_str()) {
            return Err(format!("duplicate step_id {}", p.step_id));
        }
        seen.push(&p.step_id);
    }
    if phases.is_empty() {
        return Err("at least one phase".into());
    }
    Ok(phases.to_vec())
}

/// Reads `{"phases": [{"step_id", "title", "question"}, ...]}` (the format of spec/phases.json).
pub fn phases_from_value(doc: &Value) -> Result<Vec<Phase>, String> {
    let ph = match doc.as_obj().map(|o| o.at("phases")) {
        Some(Value::Arr(ph)) if !ph.is_empty() => ph,
        _ => return Err("a phases file is {\"phases\": [{\"step_id\", \"title\", \"question\"}, ...]}".into()),
    };
    let mut out = vec![];
    for p in ph {
        let po = p.as_obj().ok_or("each phase is an object {\"step_id\", \"title\", \"question\"}")?;
        match (po.at("step_id").as_str(), po.at("title").as_str(), po.at("question").as_str()) {
            (Some(s), Some(t), Some(q)) => out.push(Phase::new(s, t, q)),
            _ => return Err("each phase is (step_id, title, question), three non-blank strings".into()),
        }
    }
    check_phases(&out)
}

/// Reads a phases file.
pub fn load_phases(path: &str) -> Result<Vec<Phase>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    phases_from_value(&crate::json::parse(&text)?)
}

/// The options of a council. `Options::default()` gives the reference's defaults.
#[derive(Clone, Debug)]
pub struct Options {
    /// Empty: `DEFAULT_ROSTER`.
    pub roster: Vec<String>,
    /// Empty: `default_phases()`.
    pub phases: Vec<Phase>,
    /// Source ids the views may cite.
    pub sources: Vec<String>,
    /// 1 to 10.
    pub max_loops: i64,
    /// 0 to 5.
    pub retries: i64,
}

impl Default for Options {
    fn default() -> Options {
        Options { roster: vec![], phases: vec![], sources: vec![], max_loops: 1, retries: 1 }
    }
}

/// A council: the phases, run with the roster, through one adapter.
pub struct Council<A: Adapter> {
    pub idea: String,
    pub adapter: A,
    pub roster: Vec<String>,
    pub phases: Vec<Phase>,
    pub sources: Vec<String>,
    pub max_loops: i64,
    pub retries: i64,
}

/// The recorded text of an adapter error: `"<name>: <message>"`, unpaired surrogates as `?`, secrets
/// redacted, at most 500 code points.
pub fn error_text(e: &AdapterError) -> String {
    let msg = replace_lone_surrogates(&e.message, "?");
    py::head(&redact(&format!("{}: {}", e.name, msg)), 500)
}

impl<A: Adapter> Council<A> {
    /// Validates the inputs: a non-blank idea; no unpaired surrogates in the idea, roster, phases or
    /// sources; unique non-blank roster names; valid phases; `max_loops` 1 to 10; `retries` 0 to 5.
    pub fn new(idea: &str, adapter: A, opts: Options) -> Result<Council<A>, String> {
        if blank_str(idea) {
            return Err("the idea must be a non-empty string".into());
        }
        let phase_texts = opts.phases.iter().flat_map(|p| [&p.step_id, &p.title, &p.question]);
        if has_lone_surrogate(idea)
            || opts.roster.iter().chain(opts.sources.iter()).chain(phase_texts).any(|t| has_lone_surrogate(t))
        {
            return Err("the idea, roster, phases and sources must not contain unpaired surrogates".into());
        }
        let roster: Vec<String> =
            if opts.roster.is_empty() { DEFAULT_ROSTER.iter().map(|s| s.to_string()).collect() } else { opts.roster };
        for (i, r) in roster.iter().enumerate() {
            if blank_str(r) || roster[..i].contains(r) {
                return Err("roster names must be unique non-empty strings".into());
            }
        }
        let phases = if opts.phases.is_empty() { default_phases() } else { check_phases(&opts.phases)? };
        if !(1..=10).contains(&opts.max_loops) {
            return Err("max_loops is an integer from 1 to 10".into());
        }
        if !(0..=5).contains(&opts.retries) {
            return Err("retries is an integer from 0 to 5".into());
        }
        Ok(Council {
            idea: idea.into(),
            adapter,
            roster,
            phases,
            sources: opts.sources,
            max_loops: opts.max_loops,
            retries: opts.retries,
        })
    }

    /// A valid answer, or the error of the last attempt, after 1 + retries attempts.
    fn ask(
        &mut self,
        mut call: impl FnMut(&mut A) -> Result<Value, AdapterError>,
        check: fn(&Value) -> Vec<String>,
    ) -> Result<Object, String> {
        let mut err = String::new();
        for _ in 0..1 + self.retries {
            let adapter = &mut self.adapter;
            let got = match catch_unwind(AssertUnwindSafe(|| call(adapter))) {
                Ok(r) => r,
                Err(p) => {
                    let msg = p
                        .downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| p.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "(message not printable)".into());
                    Err(AdapterError::new("Panic", msg))
                }
            };
            let raw = match got {
                Ok(v) => v,
                Err(e) => {
                    err = error_text(&e);
                    continue;
                }
            };
            let ans = match plain(&raw) {
                Ok(v) => v,
                Err(e) => {
                    err = py::head(&format!("invalid: {}", e), 500);
                    continue;
                }
            };
            let problems = check(&ans);
            if problems.is_empty() {
                if let Value::Obj(o) = ans {
                    return Ok(o);
                }
            }
            let cut: Vec<String> = problems.iter().take(5).map(|p| py::head(p, 200)).collect();
            err = py::head(&format!("invalid: {}", cut.join("; ")), 500);
        }
        Err(err)
    }

    /// Runs the council.
    pub fn run(&mut self) -> CouncilResult {
        let mut res = CouncilResult {
            idea: self.idea.clone(),
            adapter: self.adapter.name(),
            synthetic: self.adapter.synthetic(),
            stakeholders: self.roster.clone(),
            sources: self.sources.clone(),
            ..Default::default()
        };
        let mut prior: Vec<Prior> = vec![];
        for ph in self.phases.clone() {
            let mut it: i64 = 0;
            loop {
                it += 1;
                let ctx = Context {
                    idea: self.idea.clone(),
                    iteration: it,
                    sources: self.sources.clone(),
                    prior: prior.clone(),
                    locked: res.locked_decisions.clone(),
                };
                let mut views = vec![];
                for role in self.roster.clone() {
                    let req = ViewRequest {
                        stakeholder: role.clone(),
                        role: role.clone(),
                        phase_id: ph.step_id.clone(),
                        question: ph.question.clone(),
                        context: ctx.clone(),
                    };
                    match self.ask(|a| a.view(&req.clone()), check_view) {
                        Err(e) => views.push(StakeholderView {
                            stakeholder: role.clone(),
                            role: role.clone(),
                            status: "failed".into(),
                            error: e,
                            ..Default::default()
                        }),
                        Ok(got) => {
                            let raw = normalize_view(&got);
                            let s = |o: &Object, k: &str| o.at(k).as_str().unwrap_or("").to_string();
                            let l = |k: &str| -> Vec<String> {
                                raw.at(k)
                                    .as_arr()
                                    .map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect())
                                    .unwrap_or_default()
                            };
                            let empty = Object::new();
                            let w = raw.at("w5h1").as_obj().unwrap_or(&empty);
                            views.push(StakeholderView {
                                stakeholder: role.clone(),
                                role: role.clone(),
                                status: "ok".into(),
                                w5h1: W5H1 {
                                    who: s(w, "who"),
                                    what: s(w, "what"),
                                    where_: s(w, "where"),
                                    when: s(w, "when"),
                                    why: s(w, "why"),
                                    how: s(w, "how"),
                                },
                                done_so_far: s(&raw, "done_so_far"),
                                missing: l("missing"),
                                devils_advocate: s(&raw, "devils_advocate"),
                                contribution: s(&raw, "contribution"),
                                sources: l("sources"),
                                error: String::new(),
                            });
                            let mut fv = raw.clone();
                            fv.set("stakeholder", role.clone());
                            for fl in flag_unsourced(&fv, &str_list(&self.sources)) {
                                if let Value::Obj(mut f) = fl {
                                    f.set("step_id", ph.step_id.clone());
                                    f.set("iteration", it);
                                    res.flags.push(f);
                                }
                            }
                        }
                    }
                }
                let ok: Vec<StakeholderView> = views.iter().filter(|v| v.status == "ok").cloned().collect();
                let juror = if ok.is_empty() {
                    JurorMerge {
                        synthesis_status: "no_views".into(),
                        error: "no valid view to merge".into(),
                        ..Default::default()
                    }
                } else {
                    let req = MergeRequest {
                        phase_id: ph.step_id.clone(),
                        question: ph.question.clone(),
                        views: ok.clone(),
                        context: ctx.clone(),
                    };
                    match self.ask(|a| a.merge(&req.clone()), check_merge) {
                        Err(e) => JurorMerge { synthesis_status: "failed".into(), error: e, ..Default::default() },
                        Ok(m) => {
                            let s = |k: &str| m.at(k).as_str().unwrap_or("").to_string();
                            let l = |k: &str| -> Vec<String> {
                                m.at(k)
                                    .as_arr()
                                    .map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect())
                                    .unwrap_or_default()
                            };
                            let mut j = JurorMerge {
                                synthesis_status: "full".into(),
                                merge_summary: s("merge_summary"),
                                decision: s("decision"),
                                resolved: l("resolved"),
                                open_issues: l("open_issues"),
                                next_steps: l("next_steps"),
                                reversals: l("reversals"),
                                spec_delta: s("spec_delta"),
                                loop_recommended: m.at("loop_recommended").as_bool().unwrap_or(false),
                                error: String::new(),
                            };
                            if !synthesis_ok(&j.to_object()) {
                                j.synthesis_status = "thin".into();
                            }
                            j
                        }
                    }
                };
                res.decision_tree.push(DecisionNode {
                    step_id: ph.step_id.clone(),
                    iteration: it,
                    branches: ok.iter().map(|v| v.contribution.clone()).collect(),
                    decision: juror.decision.clone(),
                    next_steps: juror.next_steps.clone(),
                });
                if juror.synthesis_status == "full" {
                    for r in &juror.resolved {
                        if !res.locked_decisions.contains(r) {
                            res.locked_decisions.push(r.clone());
                        }
                    }
                    prior.push(Prior { step: ph.step_id.clone(), decision: juror.decision.clone() });
                }
                let again = juror.loop_recommended && it < self.max_loops;
                res.rounds.push(CouncilRound {
                    step_id: ph.step_id.clone(),
                    step_title: ph.title.clone(),
                    iteration: it,
                    question: ph.question.clone(),
                    views,
                    juror,
                });
                if !again {
                    break;
                }
            }
        }
        res
    }
}

/// The counts of a run.
#[derive(Clone, Debug, PartialEq)]
pub struct Health {
    /// By view status, sorted.
    pub views: BTreeMap<String, i64>,
    /// By synthesis status, sorted.
    pub merges: BTreeMap<String, i64>,
    pub phases: i64,
    pub phases_decided: i64,
    /// Every phase's last iteration reached a full synthesis.
    pub decided: bool,
    pub flags: i64,
    pub synthetic: bool,
}

fn counts(m: &BTreeMap<String, i64>) -> Value {
    let mut o = Object::new();
    for (k, v) in m {
        o.set(k.clone(), *v);
    }
    Value::Obj(o)
}

impl Health {
    pub fn to_value(&self) -> Value {
        obj(vec![
            ("views", counts(&self.views)),
            ("merges", counts(&self.merges)),
            ("phases", self.phases.into()),
            ("phases_decided", self.phases_decided.into()),
            ("decided", self.decided.into()),
            ("flags", self.flags.into()),
            ("synthetic", self.synthetic.into()),
        ])
    }
}

/// The last iteration of each phase, in the order of first appearance.
pub fn last_rounds(res: &CouncilResult) -> Vec<&CouncilRound> {
    let mut out: Vec<&CouncilRound> = vec![];
    for r in &res.rounds {
        match out.iter().position(|x| x.step_id == r.step_id) {
            Some(i) => out[i] = r,
            None => out.push(r),
        }
    }
    out
}

/// The counts of a run.
pub fn health(res: &CouncilResult) -> Health {
    let mut views = BTreeMap::new();
    let mut merges = BTreeMap::new();
    for r in &res.rounds {
        for v in &r.views {
            *views.entry(v.status.clone()).or_insert(0) += 1;
        }
        *merges.entry(r.juror.synthesis_status.clone()).or_insert(0) += 1;
    }
    let last = last_rounds(res);
    let decided = last.iter().filter(|r| r.juror.synthesis_status == "full").count() as i64;
    Health {
        views,
        merges,
        phases: last.len() as i64,
        phases_decided: decided,
        decided: !last.is_empty() && decided == last.len() as i64,
        flags: res.flags.len() as i64,
        synthetic: res.synthetic,
    }
}
