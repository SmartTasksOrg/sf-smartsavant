//! Reads council exports written by the SmartSignal council into a `CouncilResult` (spec/SEMANTICS.md
//! section 7), so that the gates, the audit and the exporters work on them. Two shapes are recognised: a
//! council result (rounds with views that hold the fields) and a rounds export (views with "parsed").
//! Values of the wrong type are skipped or read as empty; the key order of the input objects is kept
//! where the output depends on it (flattened object contributions).
use std::collections::BTreeSet;

use crate::gates::{blank_str, flag_unsourced, is_fallback, synthesis_ok};
use crate::json::{obj, py_float, replace_lone_surrogates, str_list, Object, Value};
use crate::model::{CouncilResult, CouncilRound, DecisionNode, JurorMerge, StakeholderView, W5H1};
use crate::py;

const STATUS: [&str; 6] = ["full", "recovered", "offline", "failed", "thin", "no_views"];
const MAX_DEPTH: usize = 20;

fn clean(t: &str) -> String {
    replace_lone_surrogates(t, "\u{FFFD}")
}

fn text_at(v: &Value, depth: usize) -> String {
    if depth > MAX_DEPTH {
        return "[nested too deep]".into();
    }
    match v {
        Value::Null => String::new(),
        Value::Str(s) => clean(s),
        Value::Obj(o) => o
            .iter()
            .map(|(k, x)| format!("{}: {}", text_at(&Value::Str(k.clone()), depth + 1), text_at(x, depth + 1)))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Arr(a) => a.iter().map(|x| text_at(x, depth + 1)).collect::<Vec<_>>().join("; "),
        Value::Bool(b) => if *b { "true" } else { "false" }.into(),
        Value::Int(_) | Value::BigInt(_) => py::repr(v),
        Value::Float(f) => {
            if f.is_finite() {
                py_float(*f)
            } else {
                "float".into()
            }
        }
    }
}

/// Any JSON value as text (`_text` in the reference).
pub fn text(v: &Value) -> String {
    text_at(v, 0)
}

fn strs(v: &Value) -> Vec<String> {
    match v {
        Value::Null => vec![],
        Value::Str(s) => {
            if blank_str(s) {
                vec![]
            } else {
                vec![s.clone()]
            }
        }
        Value::Arr(a) => a.iter().map(text).filter(|t| !blank_str(t)).collect(),
        _ => vec![text(v)],
    }
}

/// The idea of an export as one line: a string as is; an object `{name, problem, solution, audience}`
/// as "name: problem -> solution (audience: audience)".
pub fn idea_text(idea: &Value) -> String {
    match idea {
        Value::Str(s) => s.clone(),
        Value::Obj(o) => {
            let e = Value::Str(String::new());
            let f = |k: &str| text(o.get_or(k, &e));
            let (name, prob, sol, aud) = (f("name"), f("problem"), f("solution"), f("audience"));
            let mut s = py::strip(&format!("{}: {}", name, prob), ": ");
            if !sol.is_empty() {
                s += &format!(" -> {}", sol);
            }
            if !aud.is_empty() {
                s += &format!(" (audience: {})", aud);
            }
            s
        }
        _ => text(idea),
    }
}

/// `Some("smartsavant" | "smartsignal-rounds" | "smartsignal-result")`, or `None`.
pub fn detect(doc: &Value) -> Option<&'static str> {
    let o = doc.as_obj()?;
    if o.at("schema").is_str(crate::model::SCHEMA_COUNCIL) {
        return Some("smartsavant");
    }
    let rounds = o.at("rounds").as_arr()?;
    if rounds.is_empty() || !rounds.iter().any(|r| r.as_obj().map(|r| r.has("views")).unwrap_or(false)) {
        return None;
    }
    for r in rounds {
        if let Some(views) = r.as_obj().and_then(|r| r.at("views").as_arr()) {
            for v in views {
                if let Value::Obj(vo) = v {
                    return Some(if vo.has("parsed") { "smartsignal-rounds" } else { "smartsignal-result" });
                }
            }
        }
    }
    Some("smartsignal-result")
}

fn small_int(v: &Value) -> Option<i64> {
    match v {
        Value::Int(n) if (0..1000).contains(n) => Some(*n),
        _ => None,
    }
}

/// Imports a SmartSignal council export.
pub fn from_smartsignal(doc_value: &Value) -> Result<CouncilResult, String> {
    let kind = match detect(doc_value) {
        Some(k @ ("smartsignal-rounds" | "smartsignal-result")) => k,
        _ => return Err("not a SmartSignal council export (no rounds with views)".into()),
    };
    let doc = doc_value.as_obj().unwrap();
    let empty_obj = Object::new();
    let es = Value::Str(String::new());
    let mut models: BTreeSet<String> = BTreeSet::new();
    let mut res =
        CouncilResult { idea: clean(&idea_text(doc.get_or("idea", &es))), synthetic: false, ..Default::default() };
    let mut seen: Vec<(String, i64, usize)> = vec![];
    let mut names: Vec<String> = vec![];
    for rv in doc.at("rounds").as_arr().unwrap() {
        let r = match rv {
            Value::Obj(r) => r,
            _ => continue,
        };
        let mut step = text(r.get_or("step_id", &Value::Str("step".into())));
        if step.is_empty() {
            step = "step".into();
        }
        let it = small_int(r.at("iteration")).or_else(|| small_int(r.at("round"))).map(|n| n + 1).unwrap_or(1);
        let count = match seen.iter_mut().find(|(s, i, _)| *s == step && *i == it) {
            Some(e) => {
                e.2 += 1;
                e.2
            }
            None => {
                seen.push((step.clone(), it, 1));
                1
            }
        };
        if count > 1 {
            step = format!("{}.{}", step, count);
        }
        let mut views = vec![];
        for vv in r.at("views").as_arr().map(|a| a.as_slice()).unwrap_or(&[]) {
            let v = match vv {
                Value::Obj(v) => v,
                _ => continue,
            };
            let p =
                if kind == "smartsignal-rounds" { v.get_or("parsed", vv).as_obj().unwrap_or(&empty_obj) } else { v };
            let src = text(v.get_or("source", &es));
            if py::truthy(v.at("model")) {
                models.insert(text(v.at("model")));
            }
            let w = p.at("w5h1").as_obj().unwrap_or(&empty_obj);
            let who = text(v.get_or("stakeholder", &es));
            if !names.contains(&who) {
                names.push(who.clone());
            }
            let f = |k: &str| text(w.get_or(k, &es));
            views.push(StakeholderView {
                stakeholder: who,
                role: text(v.get_or("role", &es)),
                status: if src == "offline" { "offline" } else { "ok" }.into(),
                w5h1: W5H1 {
                    who: f("who"),
                    what: f("what"),
                    where_: f("where"),
                    when: f("when"),
                    why: f("why"),
                    how: f("how"),
                },
                done_so_far: text(p.at("done_so_far")),
                missing: strs(p.at("missing")),
                devils_advocate: text(p.at("devils_advocate")),
                contribution: text(p.at("contribution")),
                sources: strs(p.at("sources")).into_iter().filter(|s| py::len(s) <= 200).collect(),
                error: String::new(),
            });
        }
        let j = r.at("juror").as_obj().unwrap_or(&empty_obj);
        if py::truthy(j.at("model")) {
            models.insert(text(j.at("model")));
        }
        let mut st = text(j.get_or("synthesis_status", &es));
        if st.is_empty() || !STATUS.contains(&st.as_str()) {
            st = "failed".into();
        }
        let resolved = strs(j.at("resolved"));
        let mut decision = text(j.at("decision"));
        if decision.is_empty() {
            decision = resolved.first().cloned().unwrap_or_default();
        }
        let mut juror = JurorMerge {
            synthesis_status: st.clone(),
            merge_summary: text(j.at("merge_summary")),
            decision,
            resolved,
            open_issues: strs(j.at("open_issues")),
            next_steps: strs(j.at("next_steps")),
            reversals: strs(j.at("reversals")),
            spec_delta: text(j.at("spec_delta")),
            loop_recommended: matches!(j.at("loop_recommended"), Value::Bool(true)),
            error: String::new(),
        };
        if st == "full" && !synthesis_ok(&juror.to_object()) {
            juror.synthesis_status = "thin".into();
            juror.error = "imported as full, but it fails the synthesis gate".into();
        }
        let mut title = text(r.get_or("step_title", &Value::Str(step.clone())));
        if title.is_empty() {
            title = step.clone();
        }
        let question = text(r.get_or("question", r.get_or("step_title", &es)));
        for x in views.iter().filter(|x| x.status == "ok") {
            let fv = obj(vec![
                ("stakeholder", x.stakeholder.clone().into()),
                ("contribution", x.contribution.clone().into()),
                ("devils_advocate", x.devils_advocate.clone().into()),
                ("done_so_far", x.done_so_far.clone().into()),
                ("missing", str_list(&x.missing)),
                ("sources", str_list(&x.sources)),
            ]);
            for fl in flag_unsourced(fv.as_obj().unwrap(), &Value::Arr(vec![])) {
                if let Value::Obj(mut f) = fl {
                    f.set("step_id", step.clone());
                    f.set("iteration", it);
                    res.flags.push(f);
                }
            }
        }
        if juror.synthesis_status == "full" {
            for d in &juror.resolved {
                if !res.locked_decisions.contains(d) {
                    res.locked_decisions.push(d.clone());
                }
            }
        }
        res.decision_tree.push(DecisionNode {
            step_id: step.clone(),
            iteration: it,
            branches: views.iter().filter(|x| x.status == "ok").map(|x| x.contribution.clone()).collect(),
            decision: juror.decision.clone(),
            next_steps: juror.next_steps.clone(),
        });
        res.rounds.push(CouncilRound { step_id: step, step_title: title, iteration: it, question, views, juror });
    }
    let any_full = res.rounds.iter().any(|r| r.juror.synthesis_status == "full");
    if let Value::Arr(decs) = doc.at("decisions") {
        if any_full && !decs.is_empty() && decs.iter().all(|d| d.as_obj().is_some()) {
            res.locked_decisions = decs
                .iter()
                .map(|d| {
                    let d = d.as_obj().unwrap();
                    format!("{}: {}", text(d.get_or("topic", &es)), text(d.get_or("decision", &es)))
                })
                .filter(|x| !is_fallback(x))
                .collect();
        }
    }
    if let Value::Arr(sh) = doc.at("stakeholders") {
        res.stakeholders = sh
            .iter()
            .map(|s| match s {
                Value::Obj(o) => text(o.get_or("name", &es)),
                _ => text(s),
            })
            .collect();
    }
    if res.stakeholders.is_empty() {
        res.stakeholders = names;
    }
    res.adapter = if models.is_empty() {
        "smartsignal".into()
    } else {
        format!("smartsignal:{}", models.into_iter().collect::<Vec<_>>().join(","))
    };
    Ok(res)
}
