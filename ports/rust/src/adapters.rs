//! The one boundary between the engine and a model, and the three adapters of the reference: the stub,
//! the scripted adapter and the replay adapter.
use std::collections::HashMap;
use std::fmt;

use crate::json::{obj, Object, Value};
use crate::model::StakeholderView;
use crate::py;
use crate::sha256::sha256_hex;

/// An earlier full synthesis, as the adapter sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Prior {
    pub step: String,
    pub decision: String,
}

/// What an adapter knows about the run. Each call gets a fresh copy.
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub idea: String,
    /// 1-based, within the phase.
    pub iteration: i64,
    pub sources: Vec<String>,
    pub prior: Vec<Prior>,
    pub locked: Vec<String>,
}

/// The input of `Adapter::view`.
#[derive(Clone, Debug)]
pub struct ViewRequest {
    pub stakeholder: String,
    pub role: String,
    pub phase_id: String,
    pub question: String,
    pub context: Context,
}

/// The input of `Adapter::merge`. `views` are the valid views of the phase, in roster order.
#[derive(Clone, Debug)]
pub struct MergeRequest {
    pub phase_id: String,
    pub question: String,
    pub views: Vec<StakeholderView>,
    pub context: Context,
}

/// An adapter failure. The engine records it as `"<name>: <message>"` (secrets redacted, at most 500
/// code points), the way the reference records a Python exception.
#[derive(Clone, Debug, PartialEq)]
pub struct AdapterError {
    pub name: String,
    pub message: String,
}

impl AdapterError {
    pub fn new(name: &str, message: impl Into<String>) -> AdapterError {
        AdapterError { name: name.into(), message: message.into() }
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}

impl std::error::Error for AdapterError {}

/// The interface a model integration implements (bindings/council.d.ts, `LLMAdapter`).
///
/// `view` returns a view answer and `merge` a merge answer, as JSON values. The engine validates every
/// answer (`check_view`, `check_merge`), retries an invalid one or an `Err`, and records a failure
/// instead of inventing content. A panic in `view` or `merge` is caught and recorded like an error named
/// `Panic`. `name` is written into council.json; `synthetic` is true for adapters whose output is not a
/// real deliberation (stubs, demos), so the trainset exporter never learns from them.
pub trait Adapter {
    fn name(&self) -> String;
    fn synthetic(&self) -> bool;
    fn view(&mut self, req: &ViewRequest) -> Result<Value, AdapterError>;
    fn merge(&mut self, req: &MergeRequest) -> Result<Value, AdapterError>;
}

// ---------------------------------------------------------------- stub

fn tag(parts: &[&str]) -> String {
    sha256_hex(&parts.join("|"))[..8].to_string()
}

/// Deterministic answers: no model, no network. The vectors pin them.
#[derive(Clone, Debug, Default)]
pub struct StubAdapter;

/// The stub's view.
pub fn stub_view(req: &ViewRequest) -> Value {
    let idea = &req.context.idea;
    let t = tag(&[&req.stakeholder, &req.phase_id, idea]);
    let p = &req.phase_id;
    obj(vec![
        (
            "w5h1",
            obj(vec![
                ("who", req.stakeholder.clone().into()),
                ("what", format!("advance {}", p).into()),
                ("where", "the target architecture".into()),
                ("when", "this iteration".into()),
                ("why", py::head(idea, 120).into()),
                ("how", format!("deterministic stub sv{}", t).into()),
            ]),
        ),
        ("done_so_far", format!("{} framed {}.", req.role, p).into()),
        ("missing", Value::Arr(vec![format!("{}: target files and modules", p).into()])),
        ("devils_advocate", format!("Risk at {}: an unproven assumption (sv{}).", p, t).into()),
        ("contribution", format!("{} proposes the {} step sv{}.", req.role, p, t).into()),
        ("sources", Value::Arr(vec![])),
    ])
}

/// The stub's merge.
pub fn stub_merge(req: &MergeRequest) -> Value {
    let p = &req.phase_id;
    let resolved: Vec<Value> = req
        .views
        .iter()
        .enumerate()
        .take(5)
        .map(|(i, v)| format!("{} {}: {}", p, i + 1, v.contribution).into())
        .collect();
    let mut issues: Vec<String> = vec![];
    for v in &req.views {
        for m in &v.missing {
            if !issues.contains(m) {
                issues.push(m.clone());
            }
        }
    }
    issues.truncate(5);
    obj(vec![
        ("merge_summary", format!("Stub juror merged {} view(s) for {}.", req.views.len(), p).into()),
        ("decision", format!("{}: proceed with the stub plan", p).into()),
        ("resolved", Value::Arr(resolved)),
        ("open_issues", Value::Arr(issues.into_iter().map(Value::Str).collect())),
        ("next_steps", Value::Arr(vec![format!("Write the {} module for the stub plan", p).into()])),
        ("reversals", Value::Arr(vec![])),
        ("spec_delta", format!("[{}] stub decision recorded.", p).into()),
        ("loop_recommended", false.into()),
    ])
}

impl Adapter for StubAdapter {
    fn name(&self) -> String {
        "stub".into()
    }
    fn synthetic(&self) -> bool {
        true
    }
    fn view(&mut self, req: &ViewRequest) -> Result<Value, AdapterError> {
        Ok(stub_view(req))
    }
    fn merge(&mut self, req: &MergeRequest) -> Result<Value, AdapterError> {
        Ok(stub_merge(req))
    }
}

// ---------------------------------------------------------------- scripted

/// Answers from a script, for tests and the conformance vectors:
///
/// ```text
/// {"views":  {"frame|1|Developer": [ANSWER, ...]},   one entry per call, in order
///  "merges": {"build|1": [{"raise": "timeout"}, ANSWER]}}
/// ```
///
/// Keys are `phase|iteration|stakeholder` for views and `phase|iteration` for merges. A key that is not
/// in the script is answered by the stub. `{"raise": m}` gives `Err(AdapterError { name: "AdapterError",
/// message: m })`; anything else is returned as the answer (valid or not: the engine checks). After a
/// key's list is used up, its last entry repeats.
#[derive(Clone, Debug, Default)]
pub struct ScriptedAdapter {
    views: Object,
    merges: Object,
    n: HashMap<String, usize>,
}

impl ScriptedAdapter {
    /// A scripted adapter from a script object (anything else: no script).
    pub fn new(script: &Value) -> ScriptedAdapter {
        let part = |k: &str| match script {
            Value::Obj(o) => o.at(k).as_obj().cloned().unwrap_or_default(),
            _ => Object::new(),
        };
        ScriptedAdapter { views: part("views"), merges: part("merges"), n: HashMap::new() }
    }

    fn next(n: &mut HashMap<String, usize>, table: &Object, key: &str) -> Result<Value, AdapterError> {
        let seq = table.at(key);
        if !py::truthy(seq) {
            return Ok(Value::Null);
        }
        let items = py::iter(seq);
        if items.is_empty() {
            return Err(AdapterError::new("TypeError", format!("the script entry for {} is not a list", key)));
        }
        let i = *n.get(key).unwrap_or(&0);
        n.insert(key.to_string(), i + 1);
        let ans = &items[i.min(items.len() - 1)];
        if let Value::Obj(o) = ans {
            if o.len() == 1 && o.has("raise") {
                return Err(AdapterError::new("AdapterError", py::to_str(o.at("raise"))));
            }
        }
        Ok(ans.clone())
    }
}

impl Adapter for ScriptedAdapter {
    fn name(&self) -> String {
        "scripted".into()
    }
    fn synthetic(&self) -> bool {
        true
    }
    fn view(&mut self, req: &ViewRequest) -> Result<Value, AdapterError> {
        let key = format!("{}|{}|{}", req.phase_id, req.context.iteration, req.stakeholder);
        if self.views.has(&key) {
            return Self::next(&mut self.n, &self.views, &key);
        }
        Ok(stub_view(req))
    }
    fn merge(&mut self, req: &MergeRequest) -> Result<Value, AdapterError> {
        let key = format!("{}|{}", req.phase_id, req.context.iteration);
        if self.merges.has(&key) {
            return Self::next(&mut self.n, &self.merges, &key);
        }
        Ok(stub_merge(req))
    }
}

// ---------------------------------------------------------------- replay

/// Replays the answers of a recorded run (a council.json written by an engine of this package). Views
/// and merges whose status was not ok / full / thin are replayed as failures (`ReplayError`), so the
/// replayed run has the same health as the original.
#[derive(Clone, Debug, Default)]
pub struct ReplayAdapter {
    synthetic: bool,
    views: HashMap<String, Object>,
    merges: HashMap<String, Object>,
}

fn replay_key(parts: Vec<Value>) -> String {
    py::repr(&Value::Arr(parts))
}

impl ReplayAdapter {
    /// A replay adapter from a council object.
    pub fn new(recorded: &Object) -> ReplayAdapter {
        let mut a = ReplayAdapter { synthetic: py::truthy(recorded.at("synthetic")), ..Default::default() };
        for rv in py::iter(recorded.at("rounds")) {
            let r = match rv {
                Value::Obj(r) => r,
                _ => continue,
            };
            let (sid, it) = (r.at("step_id").clone(), r.at("iteration").clone());
            for vv in py::iter(r.at("views")) {
                if let Value::Obj(v) = vv {
                    let k = replay_key(vec![sid.clone(), it.clone(), v.at("stakeholder").clone()]);
                    a.views.entry(k).or_insert(v);
                }
            }
            let j = r.at("juror").as_obj().cloned().unwrap_or_default();
            a.merges.insert(replay_key(vec![sid, it]), j);
        }
        a
    }

    /// Reads a council.json file.
    pub fn load(path: &str) -> Result<ReplayAdapter, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        match crate::json::parse(&text)? {
            Value::Obj(o) => Ok(ReplayAdapter::new(&o)),
            _ => Err(format!("{} is not a council object", path)),
        }
    }
}

impl Adapter for ReplayAdapter {
    fn name(&self) -> String {
        "replay".into()
    }
    fn synthetic(&self) -> bool {
        self.synthetic
    }
    fn view(&mut self, req: &ViewRequest) -> Result<Value, AdapterError> {
        let k =
            replay_key(vec![req.phase_id.clone().into(), req.context.iteration.into(), req.stakeholder.clone().into()]);
        match self.views.get(&k) {
            Some(v) if v.at("status").is_str("ok") => {
                let mut out = Object::new();
                for key in crate::schema::VIEW_KEYS {
                    out.set(key, v.at(key).clone());
                }
                Ok(Value::Obj(out))
            }
            _ => Err(AdapterError::new(
                "ReplayError",
                format!("no recorded valid view for {} at {}", req.stakeholder, req.phase_id),
            )),
        }
    }
    fn merge(&mut self, req: &MergeRequest) -> Result<Value, AdapterError> {
        let k = replay_key(vec![req.phase_id.clone().into(), req.context.iteration.into()]);
        match self.merges.get(&k) {
            Some(m) if m.at("synthesis_status").is_str("full") || m.at("synthesis_status").is_str("thin") => {
                let mut out = Object::new();
                for key in crate::schema::MERGE_KEYS {
                    match m.get(key) {
                        Some(v) => {
                            out.set(key, v.clone());
                        }
                        None => {
                            return Err(AdapterError::new(
                                "ReplayError",
                                format!("the recorded merge for {} has no {}", req.phase_id, key),
                            ))
                        }
                    }
                }
                Ok(Value::Obj(out))
            }
            _ => Err(AdapterError::new("ReplayError", format!("no recorded valid merge for {}", req.phase_id))),
        }
    }
}
