package smartsavant

// The one boundary between the engine and a model, and the three adapters of the reference: the stub,
// the scripted adapter and the replay adapter.

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
)

// Prior is an earlier full synthesis, as the adapter sees it.
type Prior struct {
	Step     string
	Decision string
}

// Context is what an adapter knows about the run. Each call gets a fresh copy.
type Context struct {
	Idea      string
	Iteration int // 1-based, within the phase
	Sources   []string
	Prior     []Prior
	Locked    []string
}

func (c Context) clone() Context {
	return Context{Idea: c.Idea, Iteration: c.Iteration, Sources: copyStrs(c.Sources),
		Prior: append([]Prior{}, c.Prior...), Locked: copyStrs(c.Locked)}
}

// ViewRequest is the input of Adapter.View.
type ViewRequest struct {
	Stakeholder, Role, PhaseID, Question string
	Context                              Context
}

// MergeRequest is the input of Adapter.Merge. Views are the valid views of the phase, in roster order.
type MergeRequest struct {
	PhaseID, Question string
	Views             []StakeholderView
	Context           Context
}

// Adapter is the interface a model integration implements (bindings/council.d.ts, LLMAdapter).
//
// View must return a view answer and Merge a merge answer: JSON data made of nil, bool, numbers, string,
// []any / []Value, map[string]any or *Object (see Plain). The engine validates every answer (CheckView,
// CheckMerge), retries an invalid one or a returned error, and records a failure instead of inventing
// content. A panic in View or Merge is recovered and recorded like an error.
//
// Name is written into council.json. Synthetic is true for adapters whose output is not a real
// deliberation (stubs, demos): the trainset exporter never learns from them.
//
// An error's name in the recorded text ("<Name>: <message>") is its ErrorName() when it has that method,
// otherwise its Go type name ("Error" for errors.New and fmt.Errorf errors).
type Adapter interface {
	Name() string
	Synthetic() bool
	View(req ViewRequest) (any, error)
	Merge(req MergeRequest) (any, error)
}

// ---------------------------------------------------------------- stub

func stubTag(parts ...string) string {
	s := ""
	for i, p := range parts {
		if i > 0 {
			s += "|"
		}
		s += p
	}
	h := sha256.Sum256([]byte(s))
	return hex.EncodeToString(h[:])[:8]
}

// StubAdapter is deterministic: no model, no network. Its answers are pinned by the vectors.
type StubAdapter struct{}

// Name is "stub".
func (StubAdapter) Name() string { return "stub" }

// Synthetic is true.
func (StubAdapter) Synthetic() bool { return true }

// View returns the stub view.
func (StubAdapter) View(req ViewRequest) (any, error) { return stubView(req), nil }

// Merge returns the stub merge.
func (StubAdapter) Merge(req MergeRequest) (any, error) { return stubMerge(req), nil }

func stubView(req ViewRequest) *Object {
	idea := req.Context.Idea
	tag := stubTag(req.Stakeholder, req.PhaseID, idea)
	return Obj(
		"w5h1", Obj("who", req.Stakeholder, "what", "advance "+req.PhaseID, "where", "the target architecture",
			"when", "this iteration", "why", Head(idea, 120), "how", "deterministic stub sv"+tag),
		"done_so_far", req.Role+" framed "+req.PhaseID+".",
		"missing", []Value{req.PhaseID + ": target files and modules"},
		"devils_advocate", "Risk at "+req.PhaseID+": an unproven assumption (sv"+tag+").",
		"contribution", req.Role+" proposes the "+req.PhaseID+" step sv"+tag+".",
		"sources", []Value{},
	)
}

func stubMerge(req MergeRequest) *Object {
	resolved := []Value{}
	for i, v := range req.Views {
		if i == 5 {
			break
		}
		resolved = append(resolved, fmt.Sprintf("%s %d: %s", req.PhaseID, i+1, v.Contribution))
	}
	issues := []Value{}
	seen := map[string]bool{}
	for _, v := range req.Views {
		for _, m := range v.Missing {
			if !seen[m] {
				seen[m] = true
				if len(issues) < 5 {
					issues = append(issues, m)
				}
			}
		}
	}
	return Obj(
		"merge_summary", fmt.Sprintf("Stub juror merged %d view(s) for %s.", len(req.Views), req.PhaseID),
		"decision", req.PhaseID+": proceed with the stub plan",
		"resolved", resolved,
		"open_issues", issues,
		"next_steps", []Value{"Write the " + req.PhaseID + " module for the stub plan"},
		"reversals", []Value{},
		"spec_delta", "["+req.PhaseID+"] stub decision recorded.",
		"loop_recommended", false,
	)
}

// ---------------------------------------------------------------- scripted

// AdapterError is the error the scripted adapter returns for a {"raise": message} entry.
type AdapterError struct{ Msg string }

func (e *AdapterError) Error() string { return e.Msg }

// ErrorName is "AdapterError".
func (e *AdapterError) ErrorName() string { return "AdapterError" }

// ScriptedAdapter answers from a script, for tests and the conformance vectors:
//
//	{"views":  {"frame|1|Developer": [ANSWER, ...]},   one entry per call, in order
//	 "merges": {"build|1": [{"raise": "timeout"}, ANSWER]}}
//
// Keys are phase|iteration|stakeholder for views and phase|iteration for merges. A key that is not in the
// script is answered by the stub. {"raise": m} returns an *AdapterError; anything else is returned as the
// answer (valid or not: the engine checks). After a key's list is used up, its last entry repeats.
type ScriptedAdapter struct {
	views, merges *Object
	n             map[string]int
}

// NewScriptedAdapter builds a scripted adapter from a script object (nil: no script).
func NewScriptedAdapter(script *Object) *ScriptedAdapter {
	a := &ScriptedAdapter{views: NewObject(), merges: NewObject(), n: map[string]int{}}
	if v, ok := script.GetOr("views", nil).(*Object); ok {
		a.views = v.Copy()
	}
	if m, ok := script.GetOr("merges", nil).(*Object); ok {
		a.merges = m.Copy()
	}
	return a
}

// Name is "scripted".
func (*ScriptedAdapter) Name() string { return "scripted" }

// Synthetic is true.
func (*ScriptedAdapter) Synthetic() bool { return true }

func (a *ScriptedAdapter) next(table *Object, key string) (any, error) {
	seq, _ := table.Get(key)
	if !Truthy(seq) {
		return nil, nil
	}
	items := pyIter(seq)
	if len(items) == 0 {
		return nil, fmt.Errorf("the script entry for %s is not a list", key)
	}
	i := a.n[key]
	a.n[key] = i + 1
	ans := items[min(i, len(items)-1)]
	if o, ok := ans.(*Object); ok && o.Len() == 1 && o.Has("raise") {
		return nil, &AdapterError{Str(o.vals["raise"])}
	}
	return DeepCopy(ans), nil
}

// View answers from the script, or as the stub.
func (a *ScriptedAdapter) View(req ViewRequest) (any, error) {
	key := fmt.Sprintf("%s|%d|%s", req.PhaseID, req.Context.Iteration, req.Stakeholder)
	if a.views.Has(key) {
		return a.next(a.views, key)
	}
	return stubView(req), nil
}

// Merge answers from the script, or as the stub.
func (a *ScriptedAdapter) Merge(req MergeRequest) (any, error) {
	key := fmt.Sprintf("%s|%d", req.PhaseID, req.Context.Iteration)
	if a.merges.Has(key) {
		return a.next(a.merges, key)
	}
	return stubMerge(req), nil
}

// ---------------------------------------------------------------- replay

// ReplayError is returned when a recorded run has no valid answer for a call.
type ReplayError struct{ Msg string }

func (e *ReplayError) Error() string { return e.Msg }

// ErrorName is "ReplayError".
func (e *ReplayError) ErrorName() string { return "ReplayError" }

// ReplayAdapter replays the answers of a recorded run (a council.json written by an engine of this
// package). Views and merges whose status was not ok / full / thin are replayed as failures, so the
// replayed run has the same health as the original.
type ReplayAdapter struct {
	synthetic bool
	views     map[string]*Object
	merges    map[string]*Object
}

func replayKey(parts ...Value) string { return Repr(parts) }

// NewReplayAdapter builds a replay adapter from a council object.
func NewReplayAdapter(recorded *Object) *ReplayAdapter {
	a := &ReplayAdapter{synthetic: Truthy(recorded.GetOr("synthetic", false)), views: map[string]*Object{},
		merges: map[string]*Object{}}
	for _, rv := range pyIter(recorded.GetOr("rounds", []Value{})) {
		r, ok := rv.(*Object)
		if !ok {
			continue
		}
		sid, it := r.GetOr("step_id", nil), r.GetOr("iteration", nil)
		for _, vv := range pyIter(r.GetOr("views", []Value{})) {
			if v, ok := vv.(*Object); ok {
				k := replayKey(sid, it, v.GetOr("stakeholder", nil))
				if _, seen := a.views[k]; !seen {
					a.views[k] = v
				}
			}
		}
		j, _ := r.GetOr("juror", NewObject()).(*Object)
		a.merges[replayKey(sid, it)] = j
	}
	return a
}

// LoadReplayAdapter reads a council.json file.
func LoadReplayAdapter(path string) (*ReplayAdapter, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	doc, err := ParseJSON(data)
	if err != nil {
		return nil, err
	}
	o, ok := doc.(*Object)
	if !ok {
		return nil, fmt.Errorf("%s is not a council object", path)
	}
	return NewReplayAdapter(o), nil
}

// Name is "replay".
func (*ReplayAdapter) Name() string { return "replay" }

// Synthetic is the recorded run's synthetic flag.
func (a *ReplayAdapter) Synthetic() bool { return a.synthetic }

// View replays the recorded view.
func (a *ReplayAdapter) View(req ViewRequest) (any, error) {
	v := a.views[replayKey(req.PhaseID, int64(req.Context.Iteration), req.Stakeholder)]
	if v == nil || v.GetOr("status", nil) != "ok" {
		return nil, &ReplayError{fmt.Sprintf("no recorded valid view for %s at %s", req.Stakeholder, req.PhaseID)}
	}
	out := NewObject()
	for _, k := range viewKeys {
		out.Set(k, DeepCopy(v.GetOr(k, nil)))
	}
	return out, nil
}

// Merge replays the recorded merge.
func (a *ReplayAdapter) Merge(req MergeRequest) (any, error) {
	m := a.merges[replayKey(req.PhaseID, int64(req.Context.Iteration))]
	st := m.GetOr("synthesis_status", nil)
	if m == nil || (st != "full" && st != "thin") {
		return nil, &ReplayError{fmt.Sprintf("no recorded valid merge for %s", req.PhaseID)}
	}
	out := NewObject()
	for _, k := range mergeKeys {
		v, ok := m.Get(k)
		if !ok {
			return nil, &ReplayError{fmt.Sprintf("the recorded merge for %s has no %s", req.PhaseID, k)}
		}
		out.Set(k, DeepCopy(v))
	}
	return out, nil
}
