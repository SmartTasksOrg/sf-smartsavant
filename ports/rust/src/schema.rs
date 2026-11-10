//! Validation of what an adapter returns, and of council files (spec/SEMANTICS.md section 3; vectors
//! spec/conformance/schema.json). The error strings and their order are part of the contract.
use crate::gates::blank_str;
use crate::json::{has_lone_surrogate, Object, Value};
use crate::py;

pub const MAX_STR: usize = 8000;
pub const MAX_DELTA: usize = 20000;
pub const MAX_ITEMS: usize = 20;
pub const MAX_ITEM: usize = 2000;
pub const MAX_DEPTH: usize = 32;
/// Code points of all strings in one answer, keys included.
pub const MAX_TOTAL: i64 = 1_000_000;

pub const W5H1_KEYS: [&str; 6] = ["who", "what", "where", "when", "why", "how"];
pub const VIEW_KEYS: [&str; 6] = ["w5h1", "done_so_far", "missing", "devils_advocate", "contribution", "sources"];
pub const MERGE_KEYS: [&str; 8] = [
    "merge_summary",
    "decision",
    "resolved",
    "open_issues",
    "next_steps",
    "reversals",
    "spec_delta",
    "loop_recommended",
];
const MERGE_REQUIRED: [&str; 7] =
    ["merge_summary", "decision", "resolved", "open_issues", "next_steps", "spec_delta", "loop_recommended"];

/// The copy of an adapter answer that is validated and used (`schema.plain` in Python): nesting deeper
/// than 32 and more than 1,000,000 code points of text (keys included) are refused with the reference's
/// messages. Every `Value` is JSON data already, so no type is refused here.
pub fn plain(v: &Value) -> Result<Value, String> {
    let mut budget = MAX_TOTAL;
    fn go(x: &Value, depth: usize, budget: &mut i64) -> Result<Value, String> {
        const TOO_MUCH: &str = "more than 1,000,000 characters of text";
        if depth > MAX_DEPTH {
            return Err("nested deeper than 32 levels".into());
        }
        match x {
            Value::Str(s) => {
                *budget -= py::len(s) as i64;
                if *budget < 0 {
                    return Err(TOO_MUCH.into());
                }
                Ok(x.clone())
            }
            Value::Arr(a) => a.iter().map(|e| go(e, depth + 1, budget)).collect::<Result<Vec<_>, _>>().map(Value::Arr),
            Value::Obj(o) => {
                let mut out = Object::new();
                for (k, e) in o.iter() {
                    *budget -= py::len(k) as i64;
                    out.set(k.clone(), go(e, depth + 1, budget)?);
                }
                if *budget < 0 {
                    return Err(TOO_MUCH.into());
                }
                Ok(Value::Obj(out))
            }
            _ => Ok(x.clone()),
        }
    }
    go(v, 0, &mut budget)
}

/// An unknown key as shown in an error: control characters dropped, unpaired surrogates as U+FFFD, then
/// at most 40 code points.
fn show_key(k: &str) -> String {
    let s: String = k.chars().filter(|&c| !((c as u32) < 32 || (127..160).contains(&(c as u32)))).collect();
    let s = crate::json::replace_lone_surrogates(&s, "\u{FFFD}"); // error text is recorded: it must stay encodable
    if py::len(&s) > 40 {
        py::head(&s, 40) + "..."
    } else {
        s
    }
}

fn check_str(errs: &mut Vec<String>, at: &str, v: &Value, limit: usize, nonempty: bool) {
    match v {
        Value::Str(s) => {
            if py::len(s) > limit {
                errs.push(format!("{} is longer than {} characters", at, limit));
            } else if s.contains('\0') {
                errs.push(format!("{} contains a NUL character", at));
            } else if has_lone_surrogate(s) {
                errs.push(format!("{} contains an unpaired surrogate", at));
            } else if nonempty && blank_str(s) {
                errs.push(format!("{} must not be empty", at));
            }
        }
        _ => errs.push(format!("{} must be a string", at)),
    }
}

fn is_source_id(s: &str) -> bool {
    (1..=200).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"._:/#@-".contains(&c))
}

fn check_list(errs: &mut Vec<String>, at: &str, v: &Value, source_ids: bool) {
    let l = match v {
        Value::Arr(l) => l,
        _ => {
            errs.push(format!("{} must be a list of strings", at));
            return;
        }
    };
    if l.len() > MAX_ITEMS {
        errs.push(format!("{} has more than {} items", at, MAX_ITEMS));
    }
    for (i, x) in l.iter().take(MAX_ITEMS + 1).enumerate() {
        match x {
            Value::Str(s) if !blank_str(s) => {
                if py::len(s) > MAX_ITEM {
                    errs.push(format!("{}[{}] is longer than {} characters", at, i, MAX_ITEM));
                } else if source_ids && !is_source_id(s) {
                    errs.push(format!("{}[{}] is not a valid source id", at, i));
                } else if s.contains('\0') {
                    errs.push(format!("{}[{}] contains a NUL character", at, i));
                } else if has_lone_surrogate(s) {
                    errs.push(format!("{}[{}] contains an unpaired surrogate", at, i));
                }
            }
            _ => errs.push(format!("{}[{}] must be a non-empty string", at, i)),
        }
    }
}

fn key_errors(errs: &mut Vec<String>, o: &Object, allowed: &[&str], required: &[&str]) -> bool {
    let mut extra: Vec<&String> = o.keys().iter().filter(|k| !allowed.contains(&k.as_str())).collect();
    if !extra.is_empty() {
        extra.sort();
        let shown: Vec<String> = extra.iter().take(5).map(|k| show_key(k)).collect();
        errs.push(format!("unknown keys ({}): {}", extra.len(), shown.join(", ")));
    }
    let mut missing: Vec<&str> = required.iter().copied().filter(|k| !o.has(k)).collect();
    if !missing.is_empty() {
        missing.sort();
        errs.push(format!("missing keys: {}", missing.join(", ")));
        return false;
    }
    true
}

fn has_exactly(o: &Object, keys: &[&str]) -> bool {
    o.len() == keys.len() && keys.iter().all(|k| o.has(k))
}

/// The errors of a view answer (empty when valid), first problem first.
pub fn check_view(v: &Value) -> Vec<String> {
    let o = match v {
        Value::Obj(o) => o,
        _ => return vec!["a view must be a JSON object".into()],
    };
    let mut errs = vec![];
    if !key_errors(&mut errs, o, &VIEW_KEYS, &VIEW_KEYS) {
        return errs;
    }
    match o.at("w5h1") {
        Value::Obj(w) if has_exactly(w, &W5H1_KEYS) => {
            for k in W5H1_KEYS {
                check_str(&mut errs, &format!("w5h1.{}", k), w.at(k), MAX_STR, false);
            }
        }
        _ => errs.push("w5h1 must be an object with exactly who, what, where, when, why, how".into()),
    }
    check_str(&mut errs, "done_so_far", o.at("done_so_far"), MAX_STR, false);
    if let Value::Str(_) = o.at("missing") {
        check_str(&mut errs, "missing", o.at("missing"), MAX_STR, false);
    } else {
        check_list(&mut errs, "missing", o.at("missing"), false);
    }
    check_str(&mut errs, "devils_advocate", o.at("devils_advocate"), MAX_STR, true);
    check_str(&mut errs, "contribution", o.at("contribution"), MAX_STR, false);
    check_list(&mut errs, "sources", o.at("sources"), true);
    errs
}

/// The errors of a merge answer (empty when valid), first problem first.
pub fn check_merge(m: &Value) -> Vec<String> {
    let o = match m {
        Value::Obj(o) => o,
        _ => return vec!["a merge must be a JSON object".into()],
    };
    let mut errs = vec![];
    if !key_errors(&mut errs, o, &MERGE_KEYS, &MERGE_REQUIRED) {
        return errs;
    }
    check_str(&mut errs, "merge_summary", o.at("merge_summary"), MAX_STR, true);
    check_str(&mut errs, "decision", o.at("decision"), MAX_STR, false);
    for k in ["resolved", "open_issues", "next_steps"] {
        check_list(&mut errs, k, o.at(k), false);
    }
    if o.has("reversals") {
        check_list(&mut errs, "reversals", o.at("reversals"), false);
    }
    check_str(&mut errs, "spec_delta", o.at("spec_delta"), MAX_DELTA, false);
    if o.at("loop_recommended").as_bool().is_none() {
        errs.push("loop_recommended must be true or false".into());
    }
    errs
}

/// A copy of a valid view with `missing` always a list.
pub fn normalize_view(v: &Object) -> Object {
    let mut out = v.clone();
    if let Value::Str(s) = v.at("missing") {
        let l = if blank_str(s) { vec![] } else { vec![Value::Str(s.clone())] };
        out.set("missing", l);
    }
    out
}

// ---------------------------------------------------------------- council files

const STATUS_V: [&str; 3] = ["ok", "failed", "offline"];
const STATUS_J: [&str; 6] = ["full", "thin", "recovered", "failed", "offline", "no_views"];
const TOP: [&str; 10] = [
    "schema",
    "idea",
    "adapter",
    "synthetic",
    "stakeholders",
    "sources",
    "rounds",
    "decision_tree",
    "locked_decisions",
    "flags",
];
const VIEW_REC: [(&str, char); 10] = [
    ("stakeholder", 's'),
    ("role", 's'),
    ("status", 's'),
    ("w5h1", 'w'),
    ("done_so_far", 's'),
    ("missing", 'l'),
    ("devils_advocate", 's'),
    ("contribution", 's'),
    ("sources", 'l'),
    ("error", 's'),
];
const JUROR_REC: [(&str, char); 10] = [
    ("synthesis_status", 's'),
    ("merge_summary", 's'),
    ("decision", 's'),
    ("resolved", 'l'),
    ("open_issues", 'l'),
    ("next_steps", 'l'),
    ("reversals", 'l'),
    ("spec_delta", 's'),
    ("loop_recommended", 'b'),
    ("error", 's'),
];
const ROUND_REC: [(&str, char); 6] =
    [("step_id", 's'), ("step_title", 's'), ("iteration", 'i'), ("question", 's'), ("views", 'L'), ("juror", 'J')];
const NODE_REC: [(&str, char); 5] =
    [("step_id", 's'), ("iteration", 'i'), ("branches", 'l'), ("decision", 's'), ("next_steps", 'l')];

fn good_str(v: &Value) -> bool {
    matches!(v, Value::Str(s) if !has_lone_surrogate(s))
}

fn good_str_list(v: &Value) -> bool {
    matches!(v, Value::Arr(l) if l.iter().all(good_str))
}

fn type_ok(kind: char, v: &Value) -> bool {
    match kind {
        's' => good_str(v),
        'i' => matches!(v, Value::Int(n) if *n >= 1),
        'b' => matches!(v, Value::Bool(_)),
        'l' => good_str_list(v),
        'w' => matches!(v, Value::Obj(o) if has_exactly(o, &W5H1_KEYS) && o.iter().all(|(_, x)| good_str(x))),
        'L' => matches!(v, Value::Arr(_)),
        'J' => matches!(v, Value::Obj(_)),
        _ => false,
    }
}

fn typed(errs: &mut Vec<String>, at: &str, d: &Value, spec: &[(&str, char)]) {
    let names: Vec<&str> = spec.iter().map(|f| f.0).collect();
    match d {
        Value::Obj(o) if has_exactly(o, &names) => {
            for (k, t) in spec {
                if !type_ok(*t, o.at(k)) {
                    errs.push(format!("{}.{} has the wrong type", at, k));
                }
            }
        }
        _ => errs.push(format!("{} must have exactly the keys {}", at, names.join(", "))),
    }
}

/// Checks a council.json written by an engine of this package: exact keys and JSON types everywhere,
/// known statuses, no unpaired surrogates. At most 50 errors.
pub fn check_council(d: &Value) -> Vec<String> {
    let o = match d {
        Value::Obj(o) => o,
        _ => return vec!["a council file must be a JSON object".into()],
    };
    if !o.at("schema").is_str("smartsavant.council/1") {
        return vec!["schema must be \"smartsavant.council/1\"".into()];
    }
    if !has_exactly(o, &TOP) {
        let mut t = TOP.to_vec();
        t.sort();
        return vec![format!("top-level keys must be exactly {}", t.join(", "))];
    }
    let mut errs = vec![];
    for k in ["idea", "adapter"] {
        if !good_str(o.at(k)) {
            errs.push(format!("{} must be a string", k));
        }
    }
    if o.at("synthetic").as_bool().is_none() {
        errs.push("synthetic must be true or false".into());
    }
    for k in ["stakeholders", "sources", "locked_decisions"] {
        if !good_str_list(o.at(k)) {
            errs.push(format!("{} must be a list of strings", k));
        }
    }
    let (rounds, tree, flags) = match (o.at("rounds"), o.at("decision_tree"), o.at("flags")) {
        (Value::Arr(r), Value::Arr(t), Value::Arr(f)) => (r, t, f),
        _ => {
            errs.push("rounds, decision_tree and flags must be lists".into());
            return errs;
        }
    };
    if tree.len() != rounds.len() {
        errs.push("decision_tree must have one node per round".into());
    }
    for (i, r) in rounds.iter().enumerate() {
        let w = format!("rounds[{}]", i);
        typed(&mut errs, &w, r, &ROUND_REC);
        let ro = match r {
            Value::Obj(ro) => ro,
            _ => continue,
        };
        let (views, juror) = match (ro.at("views"), ro.at("juror")) {
            (Value::Arr(v), Value::Obj(j)) => (v, j),
            _ => continue,
        };
        for (j, v) in views.iter().enumerate() {
            typed(&mut errs, &format!("{}.views[{}]", w, j), v, &VIEW_REC);
            if let Value::Obj(vo) = v {
                if !vo.at("status").as_str().map(|s| STATUS_V.contains(&s)).unwrap_or(false) {
                    errs.push(format!("{}.views[{}].status must be one of {}", w, j, STATUS_V.join(", ")));
                }
            }
        }
        typed(&mut errs, &format!("{}.juror", w), &Value::Obj(juror.clone()), &JUROR_REC);
        if !juror.at("synthesis_status").as_str().map(|s| STATUS_J.contains(&s)).unwrap_or(false) {
            errs.push(format!("{}.juror.synthesis_status must be one of {}", w, STATUS_J.join(", ")));
        }
    }
    for (i, n) in tree.iter().enumerate() {
        typed(&mut errs, &format!("decision_tree[{}]", i), n, &NODE_REC);
    }
    for (i, f) in flags.iter().enumerate() {
        let good = matches!(f, Value::Obj(fo) if fo.iter().all(|(_, x)| good_str(x) || matches!(x, Value::Int(_))));
        if !good {
            errs.push(format!("flags[{}] must be an object of strings and integers", i));
        }
    }
    errs.truncate(50);
    errs
}
