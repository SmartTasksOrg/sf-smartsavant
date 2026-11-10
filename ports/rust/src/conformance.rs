//! The vector runner (spec/PORTING.md): the same checks as src/sf_smartsavant/conformance.py, one check
//! per case and, for runs.json, one check per expected field.
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::adapters::ScriptedAdapter;
use crate::audit::{audit_result, audit_stories, audit_trainset};
use crate::engine::{health, phases_from_value, Council, Options};
use crate::exporters::{run_digest, to_mermaid, to_spec_md, to_stories, to_trainset};
use crate::gates;
use crate::importer::from_smartsignal;
use crate::json::{canonical, equal, obj, parse, py_dumps, Value};
use crate::py;
use crate::schema::{check_merge, check_view};
use crate::sha256::sha256_hex;

fn load(dir: &str, name: &str) -> Result<Value, String> {
    let path = std::path::Path::new(dir).join(name);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    parse(&text).map_err(|e| format!("{}: {}", name, e))
}

fn strs(xs: Vec<String>) -> Value {
    Value::Arr(xs.into_iter().map(Value::Str).collect())
}

fn cases<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.as_obj().and_then(|o| o.at(k).as_arr()).map(|a| a.as_slice()).unwrap_or(&[])
}

fn field<'a>(c: &'a Value, k: &str) -> &'a Value {
    c.as_obj().map(|o| o.at(k)).unwrap_or(&crate::json::NULL)
}

fn name(c: &Value) -> String {
    field(c, "name").as_str().unwrap_or("").to_string()
}

type GateFn = fn(&Value) -> Value;

struct Runner {
    total: usize,
    fails: Vec<String>,
}

impl Runner {
    fn check(&mut self, label: String, got: impl FnOnce() -> Result<Value, String>, want: &Value) {
        self.total += 1;
        match catch_unwind(AssertUnwindSafe(got)) {
            Err(_) => self.fails.push(format!("{}: panic", label)),
            Ok(Err(e)) => self.fails.push(format!("{}: error: {}", label, e)),
            Ok(Ok(g)) => {
                if !equal(&g, want) {
                    self.fails.push(format!(
                        "{}: got {}, want {}",
                        label,
                        py::head(&py_dumps(&g, true), 300),
                        py::head(&py_dumps(want, true), 300)
                    ));
                }
            }
        }
    }
}

/// Runs every vector file in `dir`: the number of checks and the failures.
pub fn run_dir(dir: &str) -> Result<(usize, Vec<String>), String> {
    let mut r = Runner { total: 0, fails: vec![] };
    let f = load(dir, "canonical.json")?;
    for c in cases(&f, "cases") {
        r.check(
            format!("canonical/{}", name(c)),
            || {
                let t = canonical(field(c, "input"))?;
                let h = sha256_hex(&t);
                Ok(Value::Arr(vec![t.into(), h.into()]))
            },
            &Value::Arr(vec![field(c, "canonical").clone(), field(c, "sha256").clone()]),
        );
    }
    let f = load(dir, "figures.json")?;
    for (i, c) in cases(&f, "cases").iter().enumerate() {
        r.check(
            format!("figures/{}", i),
            || Ok(strs(gates::figures(field(c, "text").as_str().unwrap_or("")))),
            field(c, "figures"),
        );
    }
    let f = load(dir, "gates.json")?;
    let g = field(&f, "cases");
    for c in cases(g, "flag_unsourced") {
        r.check(
            format!("flag_unsourced/{}", name(c)),
            || {
                let view = field(c, "view").as_obj().cloned().unwrap_or_default();
                Ok(Value::Arr(gates::flag_unsourced(&view, field(c, "allowed"))))
            },
            field(c, "flags"),
        );
    }
    let fns: [(&str, GateFn); 5] = [
        ("is_generic", |t| gates::is_generic(t).into()),
        ("is_fallback", |t| gates::is_fallback(t.as_str().unwrap_or("")).into()),
        ("has_placeholder_code", |t| gates::has_placeholder_code(t.as_str().unwrap_or("")).into()),
        ("compliance_block", |t| strs(gates::compliance_block(t.as_str().unwrap_or(""), None))),
        ("squash", |t| gates::squash(t.as_str().unwrap_or("")).into()),
    ];
    for (fname, fun) in fns {
        for (i, c) in cases(g, fname).iter().enumerate() {
            r.check(format!("{}/{}", fname, i), || Ok(fun(field(c, "text"))), field(c, "result"));
        }
    }
    for c in cases(g, "synthesis_ok") {
        r.check(
            format!("synthesis_ok/{}", name(c)),
            || Ok(gates::synthesis_ok(&field(c, "merge").as_obj().cloned().unwrap_or_default()).into()),
            field(c, "result"),
        );
    }
    let f = load(dir, "schema.json")?;
    let s = field(&f, "cases");
    for c in cases(s, "view") {
        r.check(format!("schema/view/{}", name(c)), || Ok(strs(check_view(field(c, "input")))), field(c, "errors"));
    }
    for c in cases(s, "merge") {
        r.check(format!("schema/merge/{}", name(c)), || Ok(strs(check_merge(field(c, "input")))), field(c, "errors"));
    }
    let f = load(dir, "runs.json")?;
    for c in cases(&f, "cases") {
        let n = name(c);
        let list = |k: &str| -> Vec<String> {
            py::iter(field(c, k)).iter().filter_map(|x| x.as_str().map(String::from)).collect()
        };
        let mut opts = Options { roster: list("roster"), sources: list("sources"), ..Default::default() };
        let mut setup_err = None;
        if let Value::Arr(ph) = field(c, "phases") {
            if !ph.is_empty() {
                match phases_from_value(&obj(vec![("phases", Value::Arr(ph.clone()))])) {
                    Ok(p) => opts.phases = p,
                    Err(e) => setup_err = Some(e),
                }
            }
        }
        if let Some(m) = field(c, "max_loops").as_int() {
            opts.max_loops = m;
        }
        if let Some(m) = field(c, "retries").as_int() {
            opts.retries = m;
        }
        let res = match setup_err {
            Some(e) => Err(e),
            None => {
                Council::new(field(c, "idea").as_str().unwrap_or(""), ScriptedAdapter::new(field(c, "script")), opts)
                    .map(|mut council| council.run())
            }
        };
        let key = field(c, "project_key").as_str().unwrap_or("").to_string();
        if let Some(expect) = field(c, "expect").as_obj() {
            for (fieldname, want) in expect.iter() {
                r.check(
                    format!("runs/{}/{}", n, fieldname),
                    || {
                        let res = res.as_ref().map_err(|e| e.clone())?;
                        Ok(match fieldname.as_str() {
                            "council" => res.to_value(),
                            "digest" => run_digest(res).into(),
                            "health" => health(res).to_value(),
                            "stories" => to_stories(res, &key),
                            "spec_md" => to_spec_md(res).into(),
                            "mermaid" => to_mermaid(res).into(),
                            "trainset" => Value::Arr(to_trainset(res, false)),
                            "trainset_all" => Value::Arr(to_trainset(res, true)),
                            "audit" => audit_result(res).to_value(),
                            other => return Err(format!("unknown field {}", other)),
                        })
                    },
                    want,
                );
            }
        }
    }
    let f = load(dir, "audit.json")?;
    let a = field(&f, "cases");
    for c in cases(a, "import") {
        r.check(
            format!("import/{}", name(c)),
            || Ok(from_smartsignal(field(c, "input"))?.to_value()),
            field(c, "council"),
        );
    }
    for c in cases(a, "council") {
        r.check(
            format!("audit/council/{}", name(c)),
            || Ok(audit_result(&from_smartsignal(field(c, "input"))?).to_value()),
            field(c, "report"),
        );
    }
    for c in cases(a, "stories") {
        r.check(
            format!("audit/stories/{}", name(c)),
            || Ok(audit_stories(field(c, "input")).to_value()),
            field(c, "report"),
        );
    }
    for c in cases(a, "trainset") {
        r.check(
            format!("audit/trainset/{}", name(c)),
            || Ok(audit_trainset(field(c, "input").as_arr().map(|v| v.as_slice()).unwrap_or(&[])).to_value()),
            field(c, "report"),
        );
    }
    Ok((r.total, r.fails))
}
