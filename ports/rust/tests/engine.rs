//! Behaviour outside the vectors: adapter errors and panics, plain(), unpaired surrogates, replay.
use smartsavant::json::{parse, py_dumps, Value};
use smartsavant::schema::plain;
use smartsavant::{
    health, Adapter, AdapterError, Council, MergeRequest, Options, Phase, ReplayAdapter, ScriptedAdapter, StubAdapter,
    ViewRequest,
};

#[test]
fn vectors() {
    let (total, fails) = smartsavant::conformance::run_dir("../../spec/conformance").unwrap();
    assert!(fails.is_empty(), "{:?}", fails);
    assert_eq!(total, 198);
}

struct Failing(fn() -> Result<Value, AdapterError>);

impl Adapter for Failing {
    fn name(&self) -> String {
        "test".into()
    }
    fn synthetic(&self) -> bool {
        false
    }
    fn view(&mut self, _: &ViewRequest) -> Result<Value, AdapterError> {
        (self.0)()
    }
    fn merge(&mut self, req: &MergeRequest) -> Result<Value, AdapterError> {
        StubAdapter.merge(req)
    }
}

fn one_phase() -> Options {
    Options {
        roster: vec!["Dev".into()],
        phases: vec![Phase::new("frame", "Frame", "Q?")],
        retries: 0,
        ..Default::default()
    }
}

fn first_error(f: fn() -> Result<Value, AdapterError>) -> String {
    let mut c = Council::new("idea", Failing(f), one_phase()).unwrap();
    c.run().rounds[0].views[0].error.clone()
}

#[test]
fn error_text() {
    assert_eq!(first_error(|| Err(AdapterError::new("AdapterError", "x\u{10F800}y"))), "AdapterError: x?y");
    assert_eq!(
        first_error(|| Err(AdapterError::new("HTTPError", "token=abc123 failed"))),
        "HTTPError: token=*** failed"
    );
    assert_eq!(first_error(|| panic!("boom")), "Panic: boom");
    assert_eq!(
        first_error(|| Ok(parse(r#"{"w5h1": 1}"#).unwrap())),
        "invalid: missing keys: contribution, devils_advocate, done_so_far, missing, sources"
    );
    assert_eq!(first_error(|| Err(AdapterError::new("E", "é".repeat(600)))), format!("E: {}", "é".repeat(497)));
}

#[test]
fn plain_limits() {
    let mut deep = Value::Str("x".into());
    for _ in 0..40 {
        deep = Value::Arr(vec![deep]);
    }
    assert_eq!(plain(&deep).unwrap_err(), "nested deeper than 32 levels");
    let big = Value::Arr(vec![Value::Str("a".repeat(600_000)), Value::Str("b".repeat(400_001))]);
    assert_eq!(plain(&big).unwrap_err(), "more than 1,000,000 characters of text");
}

#[test]
fn surrogates() {
    let v = parse(r#"["\ud800", "🙂", "\udfff\ud800"]"#).unwrap();
    let l = v.as_arr().unwrap();
    assert!(smartsavant::json::has_lone_surrogate(l[0].as_str().unwrap()));
    assert_eq!(l[1].as_str().unwrap(), "\u{1F642}");
    assert_eq!(l[2].as_str().unwrap().chars().count(), 2);
    assert_eq!(py_dumps(&v, false), r#"["\ud800", "🙂", "\udfff\ud800"]"#);
    assert!(Council::new("a\u{10F800}", StubAdapter, Options::default()).is_err());
}

#[test]
fn replay_reproduces_a_run() {
    let opts = Options {
        roster: vec!["Dev".into(), "PO".into()],
        phases: vec![Phase::new("frame", "Frame", "Q?"), Phase::new("build", "Build", "How?")],
        ..Default::default()
    };
    let script =
        parse(r#"{"views": {"frame|1|PO": [{"raise": "down"}]}, "merges": {"build|1": [{"raise": "x"}]}}"#).unwrap();
    let res = Council::new("idea", ScriptedAdapter::new(&script), opts.clone()).unwrap().run();
    let recorded = res.to_value();
    let again = Council::new("idea", ReplayAdapter::new(recorded.as_obj().unwrap()), opts).unwrap().run();
    assert_eq!(health(&res), health(&again));
    assert_eq!(again.rounds[0].views[1].error, "ReplayError: no recorded valid view for PO at frame");
    assert_eq!(again.rounds[0].juror, res.rounds[0].juror);
    assert!(smartsavant::schema::check_council(&recorded).is_empty());
    let back = smartsavant::CouncilResult::from_value(recorded.as_obj().unwrap()).to_value();
    assert!(smartsavant::json::equal(&back, &recorded));
}
