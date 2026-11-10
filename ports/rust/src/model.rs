//! The council data model (bindings/council.d.ts, src/sf_smartsavant/model.py). `to_value()` gives the
//! JSON form with the fields in the order of the Python dataclasses.
//!
//! view.status: ok | failed | offline; juror.synthesis_status: full | thin | recovered | failed |
//! offline | no_views.
use crate::json::{obj, str_list, Object, Value};
use crate::py;

pub const SCHEMA_COUNCIL: &str = "smartsavant.council/1";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct W5H1 {
    pub who: String,
    pub what: String,
    pub where_: String,
    pub when: String,
    pub why: String,
    pub how: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StakeholderView {
    pub stakeholder: String,
    pub role: String,
    pub status: String,
    pub w5h1: W5H1,
    pub done_so_far: String,
    pub missing: Vec<String>,
    pub devils_advocate: String,
    pub contribution: String,
    pub sources: Vec<String>,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct JurorMerge {
    pub synthesis_status: String,
    pub merge_summary: String,
    pub decision: String,
    pub resolved: Vec<String>,
    pub open_issues: Vec<String>,
    pub next_steps: Vec<String>,
    pub reversals: Vec<String>,
    pub spec_delta: String,
    pub loop_recommended: bool,
    pub error: String,
}

#[derive(Clone, Debug, Default)]
pub struct CouncilRound {
    pub step_id: String,
    pub step_title: String,
    pub iteration: i64,
    pub question: String,
    pub views: Vec<StakeholderView>,
    pub juror: JurorMerge,
}

#[derive(Clone, Debug, Default)]
pub struct DecisionNode {
    pub step_id: String,
    pub iteration: i64,
    pub branches: Vec<String>,
    pub decision: String,
    pub next_steps: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CouncilResult {
    pub schema: String,
    pub idea: String,
    pub adapter: String,
    pub synthetic: bool,
    pub stakeholders: Vec<String>,
    pub sources: Vec<String>,
    pub rounds: Vec<CouncilRound>,
    pub decision_tree: Vec<DecisionNode>,
    pub locked_decisions: Vec<String>,
    /// Objects `{flag, figure | source, stakeholder, step_id, iteration}`.
    pub flags: Vec<Object>,
}

impl Default for CouncilResult {
    fn default() -> Self {
        CouncilResult {
            schema: SCHEMA_COUNCIL.into(),
            idea: String::new(),
            adapter: String::new(),
            synthetic: false,
            stakeholders: vec![],
            sources: vec![],
            rounds: vec![],
            decision_tree: vec![],
            locked_decisions: vec![],
            flags: vec![],
        }
    }
}

impl W5H1 {
    pub fn to_value(&self) -> Value {
        obj(vec![
            ("who", self.who.clone().into()),
            ("what", self.what.clone().into()),
            ("where", self.where_.clone().into()),
            ("when", self.when.clone().into()),
            ("why", self.why.clone().into()),
            ("how", self.how.clone().into()),
        ])
    }
}

impl StakeholderView {
    pub fn to_value(&self) -> Value {
        obj(vec![
            ("stakeholder", self.stakeholder.clone().into()),
            ("role", self.role.clone().into()),
            ("status", self.status.clone().into()),
            ("w5h1", self.w5h1.to_value()),
            ("done_so_far", self.done_so_far.clone().into()),
            ("missing", str_list(&self.missing)),
            ("devils_advocate", self.devils_advocate.clone().into()),
            ("contribution", self.contribution.clone().into()),
            ("sources", str_list(&self.sources)),
            ("error", self.error.clone().into()),
        ])
    }
}

impl JurorMerge {
    pub fn to_value(&self) -> Value {
        obj(vec![
            ("synthesis_status", self.synthesis_status.clone().into()),
            ("merge_summary", self.merge_summary.clone().into()),
            ("decision", self.decision.clone().into()),
            ("resolved", str_list(&self.resolved)),
            ("open_issues", str_list(&self.open_issues)),
            ("next_steps", str_list(&self.next_steps)),
            ("reversals", str_list(&self.reversals)),
            ("spec_delta", self.spec_delta.clone().into()),
            ("loop_recommended", self.loop_recommended.into()),
            ("error", self.error.clone().into()),
        ])
    }
    /// The JSON form as an object (for the gates).
    pub fn to_object(&self) -> Object {
        match self.to_value() {
            Value::Obj(o) => o,
            _ => unreachable!(),
        }
    }
}

impl CouncilRound {
    pub fn to_value(&self) -> Value {
        obj(vec![
            ("step_id", self.step_id.clone().into()),
            ("step_title", self.step_title.clone().into()),
            ("iteration", self.iteration.into()),
            ("question", self.question.clone().into()),
            ("views", Value::Arr(self.views.iter().map(|v| v.to_value()).collect())),
            ("juror", self.juror.to_value()),
        ])
    }
}

impl DecisionNode {
    pub fn to_value(&self) -> Value {
        obj(vec![
            ("step_id", self.step_id.clone().into()),
            ("iteration", self.iteration.into()),
            ("branches", str_list(&self.branches)),
            ("decision", self.decision.clone().into()),
            ("next_steps", str_list(&self.next_steps)),
        ])
    }
}

impl CouncilResult {
    /// The JSON form (council.json).
    pub fn to_value(&self) -> Value {
        obj(vec![
            ("schema", self.schema.clone().into()),
            ("idea", self.idea.clone().into()),
            ("adapter", self.adapter.clone().into()),
            ("synthetic", self.synthetic.into()),
            ("stakeholders", str_list(&self.stakeholders)),
            ("sources", str_list(&self.sources)),
            ("rounds", Value::Arr(self.rounds.iter().map(|r| r.to_value()).collect())),
            ("decision_tree", Value::Arr(self.decision_tree.iter().map(|n| n.to_value()).collect())),
            ("locked_decisions", str_list(&self.locked_decisions)),
            ("flags", Value::Arr(self.flags.iter().map(|f| Value::Obj(f.clone())).collect())),
        ])
    }

    /// The inverse of `to_value`, for a council.json that passed `check_council`.
    pub fn from_value(d: &Object) -> CouncilResult {
        let s = |o: &Object, k: &str| o.at(k).as_str().unwrap_or("").to_string();
        let l = |o: &Object, k: &str| -> Vec<String> {
            py::iter(o.at(k)).iter().filter_map(|x| x.as_str().map(String::from)).collect()
        };
        let empty = Object::new();
        let mut res = CouncilResult {
            schema: s(d, "schema"),
            idea: s(d, "idea"),
            adapter: s(d, "adapter"),
            synthetic: d.at("synthetic").as_bool().unwrap_or(false),
            stakeholders: l(d, "stakeholders"),
            sources: l(d, "sources"),
            locked_decisions: l(d, "locked_decisions"),
            ..Default::default()
        };
        for rv in py::iter(d.at("rounds")) {
            let r = rv.as_obj().unwrap_or(&empty);
            let j = r.at("juror").as_obj().unwrap_or(&empty);
            let mut rnd = CouncilRound {
                step_id: s(r, "step_id"),
                step_title: s(r, "step_title"),
                iteration: r.at("iteration").as_int().unwrap_or(0),
                question: s(r, "question"),
                ..Default::default()
            };
            for vv in py::iter(r.at("views")) {
                let v = vv.as_obj().unwrap_or(&empty);
                let w = v.at("w5h1").as_obj().unwrap_or(&empty);
                rnd.views.push(StakeholderView {
                    stakeholder: s(v, "stakeholder"),
                    role: s(v, "role"),
                    status: s(v, "status"),
                    w5h1: W5H1 {
                        who: s(w, "who"),
                        what: s(w, "what"),
                        where_: s(w, "where"),
                        when: s(w, "when"),
                        why: s(w, "why"),
                        how: s(w, "how"),
                    },
                    done_so_far: s(v, "done_so_far"),
                    missing: l(v, "missing"),
                    devils_advocate: s(v, "devils_advocate"),
                    contribution: s(v, "contribution"),
                    sources: l(v, "sources"),
                    error: s(v, "error"),
                });
            }
            rnd.juror = JurorMerge {
                synthesis_status: s(j, "synthesis_status"),
                merge_summary: s(j, "merge_summary"),
                decision: s(j, "decision"),
                resolved: l(j, "resolved"),
                open_issues: l(j, "open_issues"),
                next_steps: l(j, "next_steps"),
                reversals: l(j, "reversals"),
                spec_delta: s(j, "spec_delta"),
                loop_recommended: j.at("loop_recommended").as_bool().unwrap_or(false),
                error: s(j, "error"),
            };
            res.rounds.push(rnd);
        }
        for nv in py::iter(d.at("decision_tree")) {
            let n = nv.as_obj().unwrap_or(&empty);
            res.decision_tree.push(DecisionNode {
                step_id: s(n, "step_id"),
                iteration: n.at("iteration").as_int().unwrap_or(0),
                branches: l(n, "branches"),
                decision: s(n, "decision"),
                next_steps: l(n, "next_steps"),
            });
        }
        for f in py::iter(d.at("flags")) {
            if let Value::Obj(fo) = f {
                res.flags.push(fo);
            }
        }
        res
    }
}
