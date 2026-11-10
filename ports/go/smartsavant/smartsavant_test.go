package smartsavant

import (
	"errors"
	"strings"
	"testing"
)

func TestVectors(t *testing.T) {
	total, fails, err := RunConformance("../../../spec/conformance")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range fails {
		t.Error(f)
	}
	if total != 198 {
		t.Errorf("%d checks, want 198", total)
	}
}

// failing returns its error for views and a fixed value for merges.
type failing struct {
	viewErr  func() (any, error)
	mergeAns any
}

func (failing) Name() string                      { return "test" }
func (failing) Synthetic() bool                   { return false }
func (f failing) View(ViewRequest) (any, error)   { return f.viewErr() }
func (f failing) Merge(MergeRequest) (any, error) { return f.mergeAns, nil }

type namedErr struct{}

func (namedErr) Error() string { return "token=abc123 failed" }

type badMsg struct{}

func (badMsg) Error() string { panic("no") }

func firstError(t *testing.T, a Adapter) string {
	t.Helper()
	opts := DefaultOptions()
	opts.Roster = []string{"Dev"}
	opts.Phases = []Phase{{"frame", "Frame", "Q?"}}
	opts.Retries = 0
	c, err := NewCouncil("idea", a, opts)
	if err != nil {
		t.Fatal(err)
	}
	return c.Run().Rounds[0].Views[0].Error
}

func TestErrorText(t *testing.T) {
	cases := []struct {
		f    func() (any, error)
		want string
	}{
		{func() (any, error) { return nil, errors.New("plain") }, "Error: plain"},
		{func() (any, error) { return nil, &AdapterError{"x\U0010F800y"} }, "AdapterError: x?y"},
		{func() (any, error) { return nil, namedErr{} }, "namedErr: token=*** failed"},
		{func() (any, error) { return nil, badMsg{} }, "badMsg: (message not printable)"},
		{func() (any, error) { panic("boom") }, "Panic: boom"},
		{func() (any, error) { return struct{}{}, nil }, "invalid: struct {} is not JSON data"},
		{func() (any, error) { return map[string]any{"w5h1": 1}, nil }, "invalid: missing keys: contribution, devils_advocate, done_so_far, missing, sources"},
		{func() (any, error) { return nil, errors.New(strings.Repeat("é", 600)) }, "Error: " + strings.Repeat("é", 493)},
	}
	for _, c := range cases {
		if got := firstError(t, failing{viewErr: c.f}); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestPlainLimits(t *testing.T) {
	var deep any = "x"
	for i := 0; i < 40; i++ {
		deep = []any{deep}
	}
	if _, err := Plain(deep); err == nil || err.Error() != "nested deeper than 32 levels" {
		t.Errorf("depth: %v", err)
	}
	if _, err := Plain([]string{strings.Repeat("a", 600000), strings.Repeat("b", 400001)}); err == nil {
		t.Error("budget not enforced")
	}
	if _, err := Plain(map[int]string{1: "a"}); err == nil || err.Error() != "object keys must be strings" {
		t.Errorf("keys: %v", err)
	}
	v, err := Plain(map[string]any{"b": 1, "a": []string{"x"}})
	if err != nil || PyDumps(v, false) != `{"a": ["x"], "b": 1}` {
		t.Errorf("map: %v %v", v, err)
	}
}

func TestSurrogates(t *testing.T) {
	v, err := ParseJSON([]byte(`["\ud800", "🙂", "\udfff\ud800"]`))
	if err != nil {
		t.Fatal(err)
	}
	l := v.([]Value)
	if !HasLoneSurrogate(l[0].(string)) || HasLoneSurrogate(l[1].(string)) || l[1] != "\U0001F642" || Len(l[2].(string)) != 2 {
		t.Errorf("parse: %q", l)
	}
	if got := PyDumps(v, false); got != `["\ud800", "🙂", "\udfff\ud800"]` {
		t.Errorf("dumps: %s", got)
	}
	if _, err := NewCouncil("a\U0010F800", StubAdapter{}, DefaultOptions()); err == nil {
		t.Error("a lone surrogate in the idea was accepted")
	}
}

func TestReplayReproducesARun(t *testing.T) {
	opts := DefaultOptions()
	opts.Roster = []string{"Dev", "PO"}
	opts.Phases = []Phase{{"frame", "Frame", "Q?"}, {"build", "Build", "How?"}}
	script, _ := ParseJSON([]byte(`{"views": {"frame|1|PO": [{"raise": "down"}]}, "merges": {"build|1": [{"raise": "x"}]}}`))
	c, _ := NewCouncil("idea", NewScriptedAdapter(script.(*Object)), opts)
	res := c.Run()
	c2, _ := NewCouncil("idea", NewReplayAdapter(res.Value()), opts)
	again := c2.Run()
	if !Equal(HealthOf(res).Value(), HealthOf(again).Value()) {
		t.Errorf("health %s vs %s", PyDumps(HealthOf(res).Value(), true), PyDumps(HealthOf(again).Value(), true))
	}
	if again.Rounds[0].Views[1].Error != "ReplayError: no recorded valid view for PO at frame" {
		t.Errorf("replay error: %q", again.Rounds[0].Views[1].Error)
	}
	if again.Rounds[0].Juror.Decision != res.Rounds[0].Juror.Decision {
		t.Error("decision differs")
	}
	if errs := CheckCouncil(res.Value()); len(errs) != 0 {
		t.Errorf("CheckCouncil: %v", errs)
	}
	if !Equal(ResultFromValue(res.Value()).Value(), res.Value()) {
		t.Error("ResultFromValue does not round-trip")
	}
}

func TestMergeSeesOnlyOkViews(t *testing.T) {
	var seen []string
	a := &recorder{seen: &seen}
	opts := DefaultOptions()
	opts.Roster = []string{"A", "B"}
	opts.Phases = []Phase{{"p", "P", "Q"}}
	c, _ := NewCouncil("idea", a, opts)
	res := c.Run()
	if len(seen) != 1 || seen[0] != "A" || res.Rounds[0].Views[1].Status != "failed" {
		t.Errorf("merge saw %v", seen)
	}
}

type recorder struct{ seen *[]string }

func (*recorder) Name() string    { return "rec" }
func (*recorder) Synthetic() bool { return false }
func (r *recorder) View(q ViewRequest) (any, error) {
	if q.Stakeholder == "B" {
		return nil, errors.New("no")
	}
	return StubAdapter{}.View(q)
}
func (r *recorder) Merge(q MergeRequest) (any, error) {
	for _, v := range q.Views {
		*r.seen = append(*r.seen, v.Stakeholder)
	}
	q.Views[0].Contribution = "changed" // the engine's copy is not affected
	return StubAdapter{}.Merge(q)
}
