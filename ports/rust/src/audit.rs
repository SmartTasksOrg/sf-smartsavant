//! Audit a council run, its stories or its trainset before anyone builds or trains from it
//! (spec/SEMANTICS.md section 7). The verdict is "build-ready" only when no finding has severity "error".
//!
//! | Code | Severity | Meaning |
//! |---|---|---|
//! | AUDIT-NO-SYNTHESIS | error | a phase's last merge is not a full synthesis |
//! | AUDIT-OFFLINE-VIEWS | error | views from an offline template (warning when fewer than half) |
//! | AUDIT-FAILED-VIEWS | warning | views the adapter never delivered validly |
//! | AUDIT-FALLBACK-TEXT | error | fallback text in summaries, decisions or spec deltas |
//! | AUDIT-TEMPLATE-VIEWS | error | the same view text repeated across 3 or more phases |
//! | AUDIT-GENERIC-STEPS | warning | next steps or story titles that name nothing (error when all are) |
//! | AUDIT-PLACEHOLDER-CODE | warning | "TODO" / `return {'ok': True}` code presented as content |
//! | AUDIT-UNSOURCED | warning | figures without a source, or sources the run did not provide |
//! | AUDIT-COMPLIANCE | error | a compliance rule fired |
//! | AUDIT-SYNTHETIC | error | a synthetic run (stub, demo) |
//! | AUDIT-TRAINSET-CONTAMINATED | error | trainset rows that teach fallback or placeholder output |
//! | AUDIT-TRAINSET-UNREADABLE | error | trainset rows that are not objects with an output or messages |
use std::collections::BTreeMap;

use crate::engine::{health, last_rounds};
use crate::gates::{
    compliance_block, has_placeholder_code, is_fallback, is_generic, is_generic_str, squash, synthesis_ok,
};
use crate::json::{obj, str_list, Value};
use crate::model::{CouncilResult, CouncilRound};
use crate::py;

pub const MAX_EXAMPLES: usize = 5;

/// One audit finding.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub code: String,
    pub severity: String,
    pub count: i64,
    pub detail: String,
    /// At most 5, each at most 200 code points.
    pub examples: Vec<String>,
}

/// An audit report.
#[derive(Clone, Debug)]
pub struct Report {
    /// council, stories or trainset
    pub kind: String,
    pub summary: Value,
    pub findings: Vec<Finding>,
    /// build-ready or not build-ready
    pub verdict: String,
    /// For an imported council: the detected shape (left out of `to_value` when `None`).
    pub source: Option<String>,
}

impl Report {
    pub fn to_value(&self) -> Value {
        let fs: Vec<Value> = self
            .findings
            .iter()
            .map(|f| {
                obj(vec![
                    ("code", f.code.clone().into()),
                    ("severity", f.severity.clone().into()),
                    ("count", f.count.into()),
                    ("detail", f.detail.clone().into()),
                    ("examples", str_list(&f.examples)),
                ])
            })
            .collect();
        let mut v = obj(vec![
            ("kind", self.kind.clone().into()),
            ("summary", self.summary.clone()),
            ("findings", Value::Arr(fs)),
            ("verdict", self.verdict.clone().into()),
        ]);
        if let (Some(s), Value::Obj(o)) = (&self.source, &mut v) {
            o.set("source", s.clone());
        }
        v
    }
}

fn finding(code: &str, severity: &str, count: usize, detail: String, examples: &[String]) -> Finding {
    Finding {
        code: code.into(),
        severity: severity.into(),
        count: count as i64,
        detail,
        examples: examples.iter().take(MAX_EXAMPLES).map(|e| py::head(e, 200)).collect(),
    }
}

fn severity_order(s: &str) -> u8 {
    match s {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    }
}

fn report(kind: &str, summary: Value, mut findings: Vec<Finding>) -> Report {
    findings.sort_by(|a, b| (severity_order(&a.severity), &a.code).cmp(&(severity_order(&b.severity), &b.code)));
    let errors = findings.iter().any(|f| f.severity == "error");
    Report {
        kind: kind.into(),
        summary,
        findings,
        verdict: if errors { "not build-ready" } else { "build-ready" }.into(),
        source: None,
    }
}

fn decided(r: &CouncilRound) -> bool {
    r.juror.synthesis_status == "full" && synthesis_ok(&r.juror.to_object())
}

/// Audits a council result.
pub fn audit_result(res: &CouncilResult) -> Report {
    let h = health(res);
    let mut out = vec![];
    let last = last_rounds(res);
    let undecided: Vec<String> = last
        .iter()
        .filter(|r| !decided(r))
        .map(|r| {
            let extra =
                if r.juror.synthesis_status == "full" { " (recorded, but it fails the synthesis gate)" } else { "" };
            format!("{}: {}{}", r.step_id, r.juror.synthesis_status, extra)
        })
        .collect();
    if !undecided.is_empty() {
        out.push(finding(
            "AUDIT-NO-SYNTHESIS",
            "error",
            undecided.len(),
            format!("{} of {} phases have no full synthesis", undecided.len(), last.len()),
            &undecided,
        ));
    }
    let pairs: Vec<_> = res.rounds.iter().flat_map(|r| r.views.iter().map(move |v| (r, v))).collect();
    let off: Vec<String> = pairs
        .iter()
        .filter(|(_, v)| v.status == "offline")
        .map(|(r, v)| format!("{}/{}", r.step_id, v.stakeholder))
        .collect();
    if !off.is_empty() {
        let sev = if 2 * off.len() >= pairs.len().max(1) { "error" } else { "warning" };
        out.push(finding(
            "AUDIT-OFFLINE-VIEWS",
            sev,
            off.len(),
            format!("{} of {} views are offline templates", off.len(), pairs.len()),
            &off,
        ));
    }
    let failed: Vec<String> = pairs
        .iter()
        .filter(|(_, v)| v.status == "failed")
        .map(|(r, v)| format!("{}/{}: {}", r.step_id, v.stakeholder, v.error))
        .collect();
    if !failed.is_empty() {
        out.push(finding(
            "AUDIT-FAILED-VIEWS",
            "warning",
            failed.len(),
            format!("{} views were never valid", failed.len()),
            &failed,
        ));
    }
    let mut fb = vec![];
    for r in &res.rounds {
        let j = &r.juror;
        let mut fields: Vec<(String, &String)> = vec![
            ("merge_summary".into(), &j.merge_summary),
            ("decision".into(), &j.decision),
            ("spec_delta".into(), &j.spec_delta),
        ];
        for (k, xs) in [("resolved", &j.resolved), ("next_steps", &j.next_steps), ("open_issues", &j.open_issues)] {
            for (i, t) in xs.iter().enumerate() {
                fields.push((format!("{}[{}]", k, i), t));
            }
        }
        for (name, t) in fields {
            if is_fallback(t) {
                fb.push(format!("{}.{}: {}", r.step_id, name, py::head(t, 80)));
            }
        }
    }
    if !fb.is_empty() {
        out.push(finding(
            "AUDIT-FALLBACK-TEXT",
            "error",
            fb.len(),
            "fallback text where a decision should be".into(),
            &fb,
        ));
    }
    // the same devil's-advocate or contribution text in several phases: a template
    let mut by_text: Vec<((String, String), Vec<String>)> = vec![];
    for r in &res.rounds {
        for v in &r.views {
            for t in [&v.devils_advocate, &v.contribution] {
                let key = (v.stakeholder.clone(), squash(t));
                if py::len(&key.1) >= 20 {
                    let step = r.step_id.split('.').next().unwrap_or("").to_string();
                    match by_text.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, steps)) => {
                            if !steps.contains(&step) {
                                steps.push(step);
                            }
                        }
                        None => by_text.push((key, vec![step])),
                    }
                }
            }
        }
    }
    let tmpl: Vec<String> =
        by_text.iter().filter(|(_, s)| s.len() >= 3).map(|(k, _)| format!("{}: {}", k.0, py::head(&k.1, 60))).collect();
    if !tmpl.is_empty() {
        out.push(finding(
            "AUDIT-TEMPLATE-VIEWS",
            "error",
            tmpl.len(),
            "the same view text appears in 3 or more phases".into(),
            &tmpl,
        ));
    }
    let steps: Vec<&String> = last.iter().flat_map(|r| r.juror.next_steps.iter()).collect();
    let gen: Vec<String> = steps.iter().filter(|s| is_generic_str(s)).map(|s| s.to_string()).collect();
    if !gen.is_empty() {
        let sev = if gen.len() == steps.len() { "error" } else { "warning" };
        out.push(finding(
            "AUDIT-GENERIC-STEPS",
            sev,
            gen.len(),
            format!("{} of {} next steps name nothing", gen.len(), steps.len()),
            &gen,
        ));
    }
    let mut ph: Vec<String> = pairs
        .iter()
        .filter(|(_, v)| has_placeholder_code(&v.contribution))
        .map(|(r, v)| format!("{}/{}", r.step_id, v.stakeholder))
        .collect();
    ph.extend(
        res.rounds
            .iter()
            .filter(|r| has_placeholder_code(&r.juror.spec_delta))
            .map(|r| format!("{}.spec_delta", r.step_id)),
    );
    if !ph.is_empty() {
        out.push(finding(
            "AUDIT-PLACEHOLDER-CODE",
            "warning",
            ph.len(),
            "placeholder code presented as content".into(),
            &ph,
        ));
    }
    if !res.flags.is_empty() {
        let ex: Vec<String> = res
            .flags
            .iter()
            .map(|x| {
                let what = if py::truthy(x.at("figure")) { x.at("figure") } else { x.at("source") };
                format!("{}/{}: {}", py::to_str(x.at("step_id")), py::to_str(x.at("stakeholder")), py::to_str(what))
            })
            .collect();
        out.push(finding(
            "AUDIT-UNSOURCED",
            "warning",
            res.flags.len(),
            "figures without a source, or unknown sources".into(),
            &ex,
        ));
    }
    let mut texts: Vec<&str> = vec![&res.idea];
    for r in &res.rounds {
        texts.extend([r.juror.merge_summary.as_str(), &r.juror.decision, &r.juror.spec_delta]);
        texts.extend(r.views.iter().map(|v| v.contribution.as_str()));
    }
    let comp = compliance_block(&texts.join(" "), None);
    if !comp.is_empty() {
        out.push(finding("AUDIT-COMPLIANCE", "error", comp.len(), "compliance rules fired".into(), &comp));
    }
    if res.synthetic || res.adapter == "stub" || res.adapter == "scripted" || res.adapter.starts_with("demo") {
        out.push(finding(
            "AUDIT-SYNTHETIC",
            "error",
            1,
            format!("adapter {} is synthetic: not a real deliberation", res.adapter),
            &[],
        ));
    }
    report("council", h.to_value(), out)
}

/// Audits a stories document (`{"stories": [...]}`).
pub fn audit_stories(doc: &Value) -> Report {
    let stories = match doc {
        Value::Obj(o) => py::iter(o.get_or("stories", &Value::Arr(vec![]))),
        _ => vec![],
    };
    let empty = Value::Str(String::new());
    let mut titles = vec![];
    let mut gen = vec![];
    let mut blocked = vec![];
    for s in &stories {
        if let Value::Obj(so) = s {
            let t = so.get_or("title", &empty);
            titles.push(t.clone());
            if is_generic(t) {
                gen.push(py::to_str(t));
            }
            if so.at("status").is_str("blocked") {
                blocked.push(py::to_str(so.get_or("story_id", &empty)));
            }
        }
    }
    let mut out = vec![];
    if !gen.is_empty() {
        let sev = if gen.len() == titles.len() { "error" } else { "warning" };
        out.push(finding(
            "AUDIT-GENERIC-STEPS",
            sev,
            gen.len(),
            format!("{} of {} story titles name nothing", gen.len(), titles.len()),
            &gen,
        ));
    }
    if !blocked.is_empty() {
        out.push(finding(
            "AUDIT-NO-SYNTHESIS",
            "error",
            blocked.len(),
            format!("{} stories are blocked (no decision)", blocked.len()),
            &blocked,
        ));
    }
    if titles.is_empty() {
        out.push(finding("AUDIT-NO-SYNTHESIS", "error", 0, "no stories".into(), &[]));
    }
    report("stories", obj(vec![("stories", titles.len().into())]), out)
}

fn row_text(row: &crate::json::Object) -> String {
    if let Value::Arr(msgs) = row.at("messages") {
        let empty = Value::Str(String::new());
        return msgs
            .iter()
            .filter_map(|m| m.as_obj())
            .filter(|m| m.at("role").is_str("assistant"))
            .map(|m| py::to_str(m.get_or("content", &empty)))
            .collect::<Vec<_>>()
            .join(" ");
    }
    py::to_str(row.get_or("output", &Value::Str(String::new())))
}

/// Audits trainset rows (parsed JSON lines; `Value::Null` for a line that is not JSON).
pub fn audit_trainset(rows: &[Value]) -> Report {
    let mut bad = vec![];
    let mut unreadable = vec![];
    let mut kinds: BTreeMap<String, i64> = BTreeMap::new();
    for (i, rv) in rows.iter().enumerate() {
        let row = match rv {
            Value::Obj(o) if matches!(o.at("output"), Value::Str(_)) || matches!(o.at("messages"), Value::Arr(_)) => o,
            _ => {
                unreadable.push(format!("row {}: not an object with an output or messages", i));
                continue;
            }
        };
        let k = match row.get("meta") {
            None => "unknown".to_string(),
            Some(Value::Obj(meta)) => meta.at("kind").as_str().unwrap_or("unknown").to_string(),
            Some(_) => "unknown".to_string(),
        };
        *kinds.entry(k.clone()).or_insert(0) += 1;
        let t = row_text(row);
        let mut why = vec![];
        if is_fallback(&t) {
            why.push("fallback");
        }
        if has_placeholder_code(&t) {
            why.push("placeholder code");
        }
        if !why.is_empty() {
            bad.push(format!("row {} ({}): {}", i, k, why.join(", ")));
        }
    }
    let mut out = vec![];
    if !unreadable.is_empty() {
        out.push(finding(
            "AUDIT-TRAINSET-UNREADABLE",
            "error",
            unreadable.len(),
            format!("{} of {} rows cannot be used", unreadable.len(), rows.len()),
            &unreadable,
        ));
    }
    if !bad.is_empty() {
        out.push(finding(
            "AUDIT-TRAINSET-CONTAMINATED",
            "error",
            bad.len(),
            format!("{} of {} rows teach fallback or placeholder output", bad.len(), rows.len()),
            &bad,
        ));
    }
    if rows.is_empty() {
        out.push(finding("AUDIT-TRAINSET-CONTAMINATED", "error", 0, "no rows".into(), &[]));
    }
    let kl: Vec<Value> = kinds.into_iter().map(|(k, n)| Value::Arr(vec![k.into(), n.into()])).collect();
    report("trainset", obj(vec![("rows", rows.len().into()), ("kinds", Value::Arr(kl))]), out)
}

pub const MAX_ROWS: i64 = 200_000;
/// Code points of all lines read.
pub const MAX_CHARS: i64 = 256 * 1024 * 1024;
/// Longer lines count as unreadable rows.
pub const MAX_LINE: usize = 1024 * 1024;
/// Lines read, empty ones included.
pub const MAX_LINES: i64 = 1_000_000;

/// Parses JSON lines (`Value::Null` for a line that is not JSON), stopping at the row, size and line
/// budgets. Invalid UTF-8 is read as U+FFFD; "\r\n" and "\r" end lines as "\n" does (Python's universal
/// newlines).
pub fn read_jsonl(data: &[u8]) -> Vec<Value> {
    let text = String::from_utf8_lossy(data).replace("\r\n", "\n").replace('\r', "\n");
    let mut rows = vec![];
    let (mut rows_left, mut chars_left, mut lines_left) = (MAX_ROWS, MAX_CHARS, MAX_LINES);
    for line in text.split_inclusive('\n') {
        chars_left -= py::len(line) as i64;
        lines_left -= 1;
        if chars_left < 0 || rows_left <= 0 || lines_left < 0 {
            break;
        }
        let line = line.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n'));
        if line.is_empty() {
            continue;
        }
        rows_left -= 1;
        if py::len(line) > MAX_LINE {
            rows.push(Value::Null);
            continue;
        }
        rows.push(crate::json::parse(line).unwrap_or(Value::Null));
    }
    rows
}

/// Audits one file: a council JSON (this package or SmartSignal), a stories JSON, or a `.jsonl`
/// trainset. Zip archives are read by the Python reference only.
pub fn audit_path(path: &str) -> Result<Report, String> {
    if path.ends_with(".zip") {
        return Err("zip trainsets are read by the Python reference only".into());
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if path.ends_with(".jsonl") {
        return Ok(audit_trainset(&read_jsonl(&data)));
    }
    let text = String::from_utf8(data).map_err(|e| e.to_string())?;
    let mut doc = crate::json::parse(&text)?;
    if let Value::Arr(l) = &doc {
        if !l.is_empty() && l.iter().all(|r| r.as_obj().map(|o| o.has("views")).unwrap_or(false)) {
            doc = obj(vec![("rounds", doc.clone())]); // a bare list of rounds
        }
    }
    match crate::importer::detect(&doc) {
        Some("smartsavant") => {
            let errs = crate::schema::check_council(&doc);
            if !errs.is_empty() {
                return Err(format!("not a valid council file: {}", errs[..errs.len().min(3)].join("; ")));
            }
            Ok(audit_result(&CouncilResult::from_value(doc.as_obj().unwrap())))
        }
        Some(kind) => {
            let mut rep = audit_result(&crate::importer::from_smartsignal(&doc)?);
            rep.source = Some(kind.into());
            Ok(rep)
        }
        None => {
            if matches!(doc.as_obj().map(|o| o.at("stories")), Some(Value::Arr(_))) {
                return Ok(audit_stories(&doc));
            }
            Err("not a council result, a council export, a stories file or a trainset".into())
        }
    }
}
