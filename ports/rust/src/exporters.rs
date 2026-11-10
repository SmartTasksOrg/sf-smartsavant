//! Exporters (spec/SEMANTICS.md section 6): a council result -> run digest, user stories, a spec in
//! Markdown, a fine-tune trainset, a Mermaid tree. Every exporter reports what the run did not decide.
use crate::engine::{health, last_rounds};
use crate::gates::{is_generic_str, squash};
use crate::json::{canonical, obj, py_dumps, str_list, Object, Value};
use crate::model::CouncilResult;
use crate::py;
use crate::sha256::sha256_hex;

/// The lowercase hex SHA-256 of the canonical JSON of the whole result.
pub fn run_digest(res: &CouncilResult) -> String {
    sha256_hex(&canonical(&res.to_value()).expect("a result holds no floats"))
}

/// `{schema, project_key, run, counts, stories}`: one story per phase, from its last iteration; ready
/// when that merge is full and has a non-generic next step, blocked otherwise.
pub fn to_stories(res: &CouncilResult, project_key: &str) -> Value {
    let mut stories = vec![];
    let (mut ready, mut blocked) = (0i64, 0i64);
    for (i, r) in last_rounds(res).iter().enumerate() {
        let j = &r.juror;
        let sid = format!("STORY-{}-{:04}", project_key, i + 1);
        let steps: Vec<String> = j.next_steps.iter().filter(|s| !is_generic_str(s)).cloned().collect();
        if j.synthesis_status == "full" && !steps.is_empty() {
            let mut ac = vec![Value::from(format!("Implements the decision: {}", j.decision))];
            ac.extend(j.resolved.iter().map(|x| Value::from(format!("Holds: {}", x))));
            ac.push("A test exercises it end to end.".into());
            stories.push(obj(vec![
                ("story_id", sid.into()),
                ("status", "ready".into()),
                ("epic", r.step_title.clone().into()),
                ("title", steps[0].clone().into()),
                ("decision", j.decision.clone().into()),
                ("sub_tasks", str_list(&steps)),
                ("acceptance_criteria", Value::Arr(ac)),
                ("open_issues", str_list(&j.open_issues)),
                ("phase", r.step_id.clone().into()),
                ("iteration", r.iteration.into()),
            ]));
            ready += 1;
        } else {
            let mut issues = j.open_issues.clone();
            if !j.error.is_empty() {
                issues.push(j.error.clone());
            }
            stories.push(obj(vec![
                ("story_id", sid.into()),
                ("status", "blocked".into()),
                ("epic", r.step_title.clone().into()),
                (
                    "title",
                    format!("Decide {}: the council did not reach a decision ({})", r.step_title, j.synthesis_status)
                        .into(),
                ),
                ("decision", "".into()),
                ("sub_tasks", Value::Arr(vec![])),
                ("acceptance_criteria", Value::Arr(vec![])),
                ("open_issues", str_list(&issues)),
                ("phase", r.step_id.clone().into()),
                ("iteration", r.iteration.into()),
            ]));
            blocked += 1;
        }
    }
    obj(vec![
        ("schema", "smartsavant.stories/1".into()),
        ("project_key", project_key.into()),
        ("run", run_digest(res).into()),
        (
            "counts",
            obj(vec![("stories", (stories.len() as i64).into()), ("ready", ready.into()), ("blocked", blocked.into())]),
        ),
        ("stories", Value::Arr(stories)),
    ])
}

/// Control characters and bidirectional controls, shown as U+FFFD.
fn bad_char(c: char) -> bool {
    let n = c as u32;
    n < 32 || (127..160).contains(&n) || (0x202A..=0x202E).contains(&n) || (0x2066..=0x2069).contains(&n)
}

/// Model text made inert in Markdown: one line (`squash`), control and bidirectional-control characters
/// as U+FFFD, `&` `<` `>` as HTML entities, and every Markdown metacharacter
/// ``\ ` * _ { } [ ] ( ) # + - . ! | ~`` backslash-escaped.
pub fn md(text: &str) -> String {
    let mut out = String::new();
    for c in squash(text).chars() {
        match c {
            c if bad_char(c) => out.push('\u{FFFD}'),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c if "\\`*_{}[]()#+-.!|~".contains(c) => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// The spec in Markdown; undecided phases are marked NOT DECIDED.
pub fn to_spec_md(res: &CouncilResult) -> String {
    let h = health(res);
    let hv = h.to_value();
    let hv = hv.as_obj().unwrap();
    let mut lines: Vec<String> = vec![
        "# Council spec".into(),
        "".into(),
        format!("*{}*", md(&res.idea)),
        "".into(),
        format!(
            "Run {} with adapter {}{}. {} of {} phases decided; views {}; merges {}; {} flag(s).",
            &run_digest(res)[..16],
            md(&res.adapter),
            if res.synthetic { " (synthetic: not a real deliberation)" } else { "" },
            h.phases_decided,
            h.phases,
            py::repr(hv.at("views")),
            py::repr(hv.at("merges")),
            h.flags
        ),
        "".into(),
    ];
    if !h.decided {
        lines.push(
            "> **Not build-ready:** at least one phase has no full synthesis. Do not build from the undecided phases; \
             re-run them with a working adapter."
                .into(),
        );
        lines.push("".into());
    }
    if !res.locked_decisions.is_empty() {
        lines.extend(["## Decisions", "", "| # | Decision |", "|---|---|"].map(String::from));
        for (i, d) in res.locked_decisions.iter().enumerate() {
            lines.push(format!("| {} | {} |", i + 1, md(d)));
        }
        lines.push("".into());
    }
    for r in last_rounds(res) {
        let j = &r.juror;
        lines.extend([
            format!("## {}", md(&r.step_title)),
            "".into(),
            format!("**Question:** {}", md(&r.question)),
            "".into(),
        ]);
        if j.synthesis_status == "full" {
            lines.extend([format!("**Decision:** {}", md(&j.decision)), "".into(), md(&j.merge_summary), "".into()]);
            if !j.next_steps.is_empty() {
                lines.extend(["**Next steps:**".to_string(), "".into()]);
                lines.extend(j.next_steps.iter().map(|s| format!("- {}", md(s))));
                lines.push("".into());
            }
        } else {
            let e = if j.error.is_empty() { String::new() } else { format!(": {}", md(&j.error)) };
            lines.extend([format!("**NOT DECIDED** ({}){}", j.synthesis_status, e), "".into()]);
        }
        if !j.open_issues.is_empty() {
            lines.extend(["**Open issues:**".to_string(), "".into()]);
            lines.extend(j.open_issues.iter().map(|s| format!("- {}", md(s))));
            lines.push("".into());
        }
        let failed: Vec<&str> = r.views.iter().filter(|v| v.status != "ok").map(|v| v.stakeholder.as_str()).collect();
        if !failed.is_empty() {
            lines.extend([format!("*No valid view from: {}.*", md(&failed.join(", "))), "".into()]);
        }
    }
    if !res.flags.is_empty() {
        lines.extend([
            "## Flags".to_string(),
            "".into(),
            format!("{} finding(s) to review before use:", res.flags.len()),
            "".into(),
        ]);
        let empty = Value::Str(String::new());
        for f in res.flags.iter().take(200) {
            let mut what = f.at("figure");
            if !py::truthy(what) {
                what = f.at("source");
            }
            if !py::truthy(what) {
                what = &empty;
            }
            lines.push(format!(
                "- {}: {} ({}, {})",
                py::to_str(f.at("flag")),
                md(&py::to_str(what)),
                md(&py::to_str(f.get_or("stakeholder", &empty))),
                md(&py::to_str(f.get_or("step_id", &empty)))
            ));
        }
        lines.push("".into());
    }
    lines.join("\n")
}

/// The facilitation guidance per default phase; used only when the round's title starts with the id.
pub const METHODOLOGY: [(&str, &str); 7] = [
    ("frame", "Establish the problem, the real users and their jobs-to-be-done, and agree on one long-term goal."),
    ("concept", "Collect solution directions, then pick the wedge: the smallest offer the users would switch for."),
    ("design", "Draw the user flow and the data flow end to end, and name every screen and every store."),
    ("build", "Decide the technical approach and the riskiest part to prove first."),
    ("integrate", "Fit the design into the target architecture: owners, interfaces, auth, deployment."),
    ("measure", "Set the KPIs, how each one is measured, and the thresholds that would change the plan."),
    ("spec", "Assemble the decisions into one document that an engineer can build from."),
];

/// The rows `{instruction, input, output, meta}`: a methodology row per default phase used; expert rows
/// (ok views without a flag) and facilitation rows (full merges) unless the run is synthetic and
/// `allow_synthetic` is false.
pub fn to_trainset(res: &CouncilResult, allow_synthetic: bool) -> Vec<Value> {
    let digest = run_digest(res);
    let mut rows = vec![];
    let flagged: Vec<String> = res
        .flags
        .iter()
        .map(|f| {
            py::repr(&Value::Arr(vec![f.at("step_id").clone(), f.at("iteration").clone(), f.at("stakeholder").clone()]))
        })
        .collect();
    let learn = allow_synthetic || !res.synthetic;
    let mut seen: Vec<&str> = vec![];
    for r in &res.rounds {
        let meta = |extra: Vec<(&str, Value)>| {
            let mut o = Object::new()
                .with("phase", r.step_id.clone())
                .with("iteration", r.iteration)
                .with("run", digest.clone())
                .with("adapter", res.adapter.clone());
            for (k, v) in extra {
                o.set(k, v);
            }
            Value::Obj(o)
        };
        if let Some((_, m)) = METHODOLOGY.iter().find(|(id, _)| *id == r.step_id) {
            if !seen.contains(&r.step_id.as_str()) && py::lower(&r.step_title).starts_with(&r.step_id) {
                seen.push(&r.step_id);
                rows.push(obj(vec![
                    (
                        "instruction",
                        format!(
                            "We are at the {} phase of a design council. How should I facilitate it?",
                            r.step_title
                        )
                        .into(),
                    ),
                    ("input", "".into()),
                    ("output", (*m).into()),
                    ("meta", meta(vec![("kind", "methodology".into())])),
                ]));
            }
        }
        if !learn {
            continue;
        }
        for v in &r.views {
            let key =
                py::repr(&Value::Arr(vec![r.step_id.clone().into(), r.iteration.into(), v.stakeholder.clone().into()]));
            if v.status == "ok" && !flagged.contains(&key) {
                let vv = v.to_value();
                let vv = vv.as_obj().unwrap();
                let mut out = Object::new();
                for k in crate::schema::VIEW_KEYS {
                    out.set(k, vv.at(k).clone());
                }
                rows.push(obj(vec![
                    (
                        "instruction",
                        format!("As the {} on a design council, give your view on: {}", v.stakeholder, r.question)
                            .into(),
                    ),
                    ("input", res.idea.clone().into()),
                    ("output", py_dumps(&Value::Obj(out), true).into()),
                    ("meta", meta(vec![("kind", "expert".into()), ("stakeholder", v.stakeholder.clone().into())])),
                ]));
            }
        }
        if r.juror.synthesis_status == "full" {
            let j = &r.juror;
            let contribs: Vec<String> =
                r.views.iter().filter(|v| v.status == "ok").map(|v| v.contribution.clone()).collect();
            let out = obj(vec![
                ("decision", j.decision.clone().into()),
                ("merge_summary", j.merge_summary.clone().into()),
                ("resolved", str_list(&j.resolved)),
                ("next_steps", str_list(&j.next_steps)),
            ]);
            rows.push(obj(vec![
                ("instruction", format!("Merge the panel for the {} phase as the juror.", r.step_title).into()),
                ("input", py_dumps(&str_list(&contribs), false).into()),
                ("output", py_dumps(&out, true).into()),
                ("meta", meta(vec![("kind", "facilitation".into()), ("synthesis", "full".into())])),
            ]));
        }
    }
    rows
}

/// A Mermaid label: squashed, control characters as U+FFFD, at most 80 code points, `&` `"` `<` `>`
/// replaced, quoted.
pub fn mermaid_label(t: &str) -> String {
    let s: String = squash(t).chars().map(|c| if bad_char(c) { '\u{FFFD}' } else { c }).collect();
    let s = py::head(&s, 80).replace('&', "&amp;").replace('"', "'").replace('<', "&lt;").replace('>', "&gt;");
    format!("\"{}\"", s)
}

/// The decision tree as a Mermaid flowchart.
pub fn to_mermaid(res: &CouncilResult) -> String {
    let mut lines = vec!["flowchart TD".to_string(), "  start([Idea])".into()];
    let mut prev = "start".to_string();
    for (i, n) in res.decision_tree.iter().enumerate() {
        let nid = format!("n{}", i);
        let r = &res.rounds[i];
        let label = if r.juror.synthesis_status == "full" {
            n.decision.clone()
        } else {
            format!("NOT DECIDED: {}", r.juror.synthesis_status)
        };
        lines.push(format!("  {}{{{}}}", nid, mermaid_label(&format!("{} #{}", r.step_title, n.iteration))));
        lines.push(format!("  {} --> {}", prev, nid));
        lines.push(format!("  {}d[{}]", nid, mermaid_label(&label)));
        lines.push(format!("  {} --> {}d", nid, nid));
        prev = nid;
    }
    lines.join("\n") + "\n"
}
