package smartsavant

// The council engine (spec/SEMANTICS.md section 4): roster -> phases -> stakeholder views -> juror merge ->
// decision tree. Every answer is validated and retried; one that still fails is recorded as failed with an
// error, never filled with invented text. Deterministic for a deterministic adapter.

import (
	"errors"
	"fmt"
	"os"
	"reflect"
	"strings"
)

// DefaultRoster is the roster used when none is given.
var DefaultRoster = []string{"Developer", "UX/UI", "Product Owner", "Architecture/Infra"}

// Phase is one step of the council.
type Phase struct {
	StepID, Title, Question string
}

// DefaultPhases are the phases of spec/phases.json.
var DefaultPhases = []Phase{
	{"frame", "Frame", "What is the problem, who are the users, and the jobs-to-be-done?"},
	{"concept", "Concept", "What is the solution direction and the wedge?"},
	{"design", "Design", "What are the user flow, data flow, and experience?"},
	{"build", "Build", "What is the technical approach?"},
	{"integrate", "Integrate", "How does it fit the target architecture?"},
	{"measure", "Measure", "What are the KPIs and success metrics?"},
	{"spec", "Spec", "Assemble the final document."},
}

// CheckPhases returns the phases when every field is a non-blank string and the step ids are unique.
func CheckPhases(phases []Phase) ([]Phase, error) {
	seen := map[string]bool{}
	out := []Phase{}
	for _, p := range phases {
		if blankStr(p.StepID) || blankStr(p.Title) || blankStr(p.Question) {
			return nil, errors.New("each phase is (step_id, title, question), three non-blank strings")
		}
		if seen[p.StepID] {
			return nil, fmt.Errorf("duplicate step_id %s", p.StepID)
		}
		seen[p.StepID] = true
		out = append(out, p)
	}
	if len(out) == 0 {
		return nil, errors.New("at least one phase")
	}
	return out, nil
}

// PhasesFromValue reads {"phases": [{"step_id", "title", "question"}, ...]} (the format of
// spec/phases.json).
func PhasesFromValue(doc Value) ([]Phase, error) {
	o, _ := doc.(*Object)
	ph, ok := o.GetOr("phases", nil).([]Value)
	if !ok || len(ph) == 0 {
		return nil, errors.New(`a phases file is {"phases": [{"step_id", "title", "question"}, ...]}`)
	}
	var out []Phase
	for _, p := range ph {
		po, ok := p.(*Object)
		if !ok {
			return nil, errors.New(`each phase is an object {"step_id", "title", "question"}`)
		}
		s, ok1 := po.GetOr("step_id", nil).(string)
		t, ok2 := po.GetOr("title", nil).(string)
		q, ok3 := po.GetOr("question", nil).(string)
		if !ok1 || !ok2 || !ok3 {
			return nil, errors.New("each phase is (step_id, title, question), three non-blank strings")
		}
		out = append(out, Phase{s, t, q})
	}
	return CheckPhases(out)
}

// LoadPhases reads a phases file.
func LoadPhases(path string) ([]Phase, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	doc, err := ParseJSON(data)
	if err != nil {
		return nil, err
	}
	return PhasesFromValue(doc)
}

// Options configure a council. Use DefaultOptions and change what you need: a zero MaxLoops is an error,
// as in the reference.
type Options struct {
	Roster   []string // empty: DefaultRoster
	Phases   []Phase  // empty: DefaultPhases
	Sources  []string // source ids the views may cite
	MaxLoops int      // 1 to 10
	Retries  int      // 0 to 5
}

// DefaultOptions are the defaults of the reference: default roster and phases, no sources, MaxLoops 1,
// Retries 1.
func DefaultOptions() Options { return Options{MaxLoops: 1, Retries: 1} }

// Council runs the phases with the roster through one adapter.
type Council struct {
	Idea     string
	Adapter  Adapter
	Roster   []string
	Phases   []Phase
	Sources  []string
	MaxLoops int
	Retries  int
}

// NewCouncil validates the inputs: a non-blank idea; no unpaired surrogates in the idea, roster, phases or
// sources; unique non-blank roster names; valid phases; MaxLoops 1 to 10; Retries 0 to 5.
func NewCouncil(idea string, adapter Adapter, opts Options) (*Council, error) {
	if blankStr(idea) {
		return nil, errors.New("the idea must be a non-empty string")
	}
	texts := append(append([]string{idea}, opts.Roster...), opts.Sources...)
	for _, p := range opts.Phases {
		texts = append(texts, p.StepID, p.Title, p.Question)
	}
	for _, t := range texts {
		if HasLoneSurrogate(t) {
			return nil, errors.New("the idea, roster, phases and sources must not contain unpaired surrogates")
		}
	}
	if adapter == nil {
		return nil, errors.New("adapter must be an LLMAdapter")
	}
	c := &Council{Idea: idea, Adapter: adapter, Sources: copyStrs(opts.Sources)}
	c.Roster = copyStrs(DefaultRoster)
	if len(opts.Roster) > 0 {
		c.Roster = copyStrs(opts.Roster)
	}
	seen := map[string]bool{}
	for _, r := range c.Roster {
		if seen[r] || blankStr(r) {
			return nil, errors.New("roster names must be unique non-empty strings")
		}
		seen[r] = true
	}
	c.Phases = append([]Phase{}, DefaultPhases...)
	if len(opts.Phases) > 0 {
		ph, err := CheckPhases(opts.Phases)
		if err != nil {
			return nil, err
		}
		c.Phases = ph
	}
	if opts.MaxLoops < 1 || opts.MaxLoops > 10 {
		return nil, errors.New("max_loops is an integer from 1 to 10")
	}
	if opts.Retries < 0 || opts.Retries > 5 {
		return nil, errors.New("retries is an integer from 0 to 5")
	}
	c.MaxLoops, c.Retries = opts.MaxLoops, opts.Retries
	return c, nil
}

const notPrintable = "(message not printable)"

func errorName(e error) string {
	if n, ok := e.(interface{ ErrorName() string }); ok {
		if name := n.ErrorName(); name != "" {
			return name
		}
	}
	t := reflect.TypeOf(e)
	for t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t.Name() == "" || t.PkgPath() == "errors" || t.PkgPath() == "fmt" {
		return "Error"
	}
	return t.Name()
}

func errorMessage(e error) (msg string) {
	defer func() {
		if recover() != nil {
			msg = notPrintable
		}
	}()
	return e.Error()
}

// ErrorText is the recorded text of an adapter error: "<Name>: <message>", with unpaired surrogates and
// invalid UTF-8 as '?', secrets redacted (Redact), at most 500 code points.
func ErrorText(e error) string {
	name := errorName(e)
	msg := ReplaceLoneSurrogates(strings.ToValidUTF8(errorMessage(e), "?"), "?")
	return Head(Redact(name+": "+msg), 500)
}

// panicError is a recovered panic of an adapter.
type panicError struct{ v any }

func (p panicError) Error() string   { return fmt.Sprint(p.v) }
func (panicError) ErrorName() string { return "Panic" }

func (c *Council) call(f func() (any, error)) (ans any, err error) {
	defer func() {
		if p := recover(); p != nil {
			ans, err = nil, panicError{p}
		}
	}()
	return f()
}

// ask returns a valid answer, or nil and the error of the last attempt, after 1 + Retries attempts.
func (c *Council) ask(f func() (any, error), check func(Value) []string) (*Object, string) {
	errText := ""
	for n := 0; n < 1+c.Retries; n++ {
		raw, err := c.call(f)
		if err != nil {
			errText = ErrorText(err)
			continue
		}
		ans, err := Plain(raw) // validate exactly what will be used
		if err != nil {
			errText = Head("invalid: "+err.Error(), 500)
			continue
		}
		problems := check(ans)
		if len(problems) == 0 {
			return ans.(*Object), ""
		}
		if len(problems) > 5 {
			problems = problems[:5]
		}
		cut := make([]string, len(problems))
		for i, p := range problems {
			cut[i] = Head(p, 200)
		}
		errText = Head("invalid: "+strings.Join(cut, "; "), 500)
	}
	return nil, errText
}

func objStrs(v Value) []string {
	l, _ := v.([]Value)
	out := make([]string, 0, len(l))
	for _, x := range l {
		s, _ := x.(string)
		out = append(out, s)
	}
	return out
}

func objStr(o *Object, k string) string { s, _ := o.GetOr(k, "").(string); return s }

// Run runs the council.
func (c *Council) Run() *CouncilResult {
	res := &CouncilResult{Schema: SchemaCouncil, Idea: c.Idea, Adapter: c.Adapter.Name(),
		Synthetic: c.Adapter.Synthetic(), Stakeholders: copyStrs(c.Roster), Sources: copyStrs(c.Sources),
		Rounds: []CouncilRound{}, DecisionTree: []DecisionNode{}, LockedDecisions: []string{}, Flags: []*Object{}}
	prior := []Prior{}
	for _, ph := range c.Phases {
		for it := 1; ; it++ {
			ctx := Context{Idea: c.Idea, Iteration: it, Sources: c.Sources, Prior: prior, Locked: res.LockedDecisions}
			views := []StakeholderView{}
			for _, role := range c.Roster {
				req := ViewRequest{Stakeholder: role, Role: role, PhaseID: ph.StepID, Question: ph.Question}
				got, errText := c.ask(func() (any, error) {
					r := req
					r.Context = ctx.clone()
					return c.Adapter.View(r)
				}, CheckView)
				if got == nil {
					views = append(views, StakeholderView{Stakeholder: role, Role: role, Status: "failed",
						Missing: []string{}, Sources: []string{}, Error: errText})
					continue
				}
				raw := NormalizeView(got)
				w := raw.vals["w5h1"].(*Object)
				views = append(views, StakeholderView{Stakeholder: role, Role: role, Status: "ok",
					W5H1: W5H1{objStr(w, "who"), objStr(w, "what"), objStr(w, "where"), objStr(w, "when"),
						objStr(w, "why"), objStr(w, "how")},
					DoneSoFar: objStr(raw, "done_so_far"), Missing: objStrs(raw.vals["missing"]),
					DevilsAdvocate: objStr(raw, "devils_advocate"), Contribution: objStr(raw, "contribution"),
					Sources: objStrs(raw.vals["sources"])})
				fv := raw.Copy().Set("stakeholder", role)
				for _, fl := range FlagUnsourced(fv, strList(c.Sources)) {
					f := fl.(*Object).Set("step_id", ph.StepID).Set("iteration", int64(it))
					res.Flags = append(res.Flags, f)
				}
			}
			var ok []StakeholderView
			for _, v := range views {
				if v.Status == "ok" {
					ok = append(ok, v)
				}
			}
			var juror JurorMerge
			if len(ok) == 0 {
				juror = JurorMerge{SynthesisStatus: "no_views", Error: "no valid view to merge"}
			} else {
				m, errText := c.ask(func() (any, error) {
					return c.Adapter.Merge(MergeRequest{PhaseID: ph.StepID, Question: ph.Question,
						Views: copyViews(ok), Context: ctx.clone()})
				}, CheckMerge)
				if m == nil {
					juror = JurorMerge{SynthesisStatus: "failed", Error: errText}
				} else {
					loop, _ := m.vals["loop_recommended"].(bool)
					juror = JurorMerge{SynthesisStatus: "full", MergeSummary: objStr(m, "merge_summary"),
						Decision: objStr(m, "decision"), Resolved: objStrs(m.vals["resolved"]),
						OpenIssues: objStrs(m.vals["open_issues"]), NextSteps: objStrs(m.vals["next_steps"]),
						Reversals: objStrs(m.GetOr("reversals", []Value{})), SpecDelta: objStr(m, "spec_delta"),
						LoopRecommended: loop}
					if !SynthesisOK(juror.Value()) {
						juror.SynthesisStatus = "thin"
					}
				}
			}
			juror = fillJuror(juror)
			res.Rounds = append(res.Rounds, CouncilRound{StepID: ph.StepID, StepTitle: ph.Title, Iteration: it,
				Question: ph.Question, Views: views, Juror: juror})
			branches := []string{}
			for _, v := range ok {
				branches = append(branches, v.Contribution)
			}
			res.DecisionTree = append(res.DecisionTree, DecisionNode{StepID: ph.StepID, Iteration: it,
				Branches: branches, Decision: juror.Decision, NextSteps: copyStrs(juror.NextSteps)})
			if juror.SynthesisStatus == "full" {
				for _, r := range juror.Resolved {
					if !contains(res.LockedDecisions, r) {
						res.LockedDecisions = append(res.LockedDecisions, r)
					}
				}
				prior = append(append([]Prior{}, prior...), Prior{ph.StepID, juror.Decision})
			}
			if !juror.LoopRecommended || it >= c.MaxLoops {
				break
			}
		}
	}
	return res
}

func fillJuror(j JurorMerge) JurorMerge {
	for _, p := range []*[]string{&j.Resolved, &j.OpenIssues, &j.NextSteps, &j.Reversals} {
		if *p == nil {
			*p = []string{}
		}
	}
	return j
}

func copyViews(vs []StakeholderView) []StakeholderView {
	out := make([]StakeholderView, len(vs))
	for i, v := range vs {
		v.Missing, v.Sources = copyStrs(v.Missing), copyStrs(v.Sources)
		out[i] = v
	}
	return out
}

// Count is one entry of a sorted count table.
type Count struct {
	Key string
	N   int
}

// Health is the counts of a run.
type Health struct {
	Views         []Count // by view status, sorted
	Merges        []Count // by synthesis status, sorted
	Phases        int
	PhasesDecided int
	Decided       bool // every phase's last iteration reached a full synthesis
	Flags         int
	Synthetic     bool
}

func countsValue(cs []Count) *Object {
	o := NewObject()
	for _, c := range cs {
		o.Set(c.Key, int64(c.N))
	}
	return o
}

// Value is the JSON form.
func (h Health) Value() *Object {
	return Obj("views", countsValue(h.Views), "merges", countsValue(h.Merges), "phases", int64(h.Phases),
		"phases_decided", int64(h.PhasesDecided), "decided", h.Decided, "flags", int64(h.Flags),
		"synthetic", h.Synthetic)
}

func countSorted(m map[string]int) []Count {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	out := []Count{}
	for _, k := range sortedStrings(keys) {
		out = append(out, Count{k, m[k]})
	}
	return out
}

// lastRounds is the last iteration of each phase, in the order of first appearance.
func lastRounds(res *CouncilResult) []CouncilRound {
	idx := map[string]int{}
	var out []CouncilRound
	for _, r := range res.Rounds {
		if i, ok := idx[r.StepID]; ok {
			out[i] = r
		} else {
			idx[r.StepID] = len(out)
			out = append(out, r)
		}
	}
	return out
}

// HealthOf returns the counts of a run.
func HealthOf(res *CouncilResult) Health {
	views, merges := map[string]int{}, map[string]int{}
	for _, r := range res.Rounds {
		for _, v := range r.Views {
			views[v.Status]++
		}
		merges[r.Juror.SynthesisStatus]++
	}
	last := lastRounds(res)
	decided := 0
	for _, r := range last {
		if r.Juror.SynthesisStatus == "full" {
			decided++
		}
	}
	return Health{Views: countSorted(views), Merges: countSorted(merges), Phases: len(last), PhasesDecided: decided,
		Decided: len(last) > 0 && decided == len(last), Flags: len(res.Flags), Synthetic: res.Synthetic}
}
