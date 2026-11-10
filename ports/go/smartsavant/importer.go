package smartsavant

// Read council exports written by the SmartSignal council into a CouncilResult (spec/SEMANTICS.md
// section 7), so that the gates, the audit and the exporters work on them. Two shapes are recognised: a
// council result (rounds with views that hold the fields) and a rounds export (views with "parsed").
// Values of the wrong type are skipped or read as empty; the key order of the input objects is kept where
// the output depends on it (flattened object contributions).

import (
	"errors"
	"fmt"
	"math"
	"strings"
)

var importStatus = []string{"full", "recovered", "offline", "failed", "thin", "no_views"}

const importMaxDepth = 20

func clean(t string) string { return ReplaceLoneSurrogates(t, "�") }

// importText is _text: any JSON value as text.
func importText(v Value, depth int) string {
	if depth > importMaxDepth {
		return "[nested too deep]"
	}
	switch x := v.(type) {
	case nil:
		return ""
	case string:
		return clean(x)
	case *Object:
		parts := make([]string, 0, x.Len())
		for _, k := range x.keys {
			parts = append(parts, importText(k, depth+1)+": "+importText(x.vals[k], depth+1))
		}
		return strings.Join(parts, "\n")
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			parts[i] = importText(e, depth+1)
		}
		return strings.Join(parts, "; ")
	case bool:
		if x {
			return "true"
		}
		return "false"
	case int64, int, BigInt:
		return Repr(x)
	case float64:
		if math.IsNaN(x) || math.IsInf(x, 0) {
			return "float"
		}
		return pyFloat(x)
	}
	return pyTypeName(v)
}

func text(v Value) string { return importText(v, 0) }

func asObject(v Value) *Object {
	if o, ok := v.(*Object); ok {
		return o
	}
	return NewObject()
}

func asList(v Value) []Value {
	if l, ok := v.([]Value); ok {
		return l
	}
	return nil
}

func importStrs(v Value) []string {
	switch x := v.(type) {
	case nil:
		return []string{}
	case string:
		if blankStr(x) {
			return []string{}
		}
		return []string{x}
	case []Value:
		out := []string{}
		for _, e := range x {
			if t := text(e); !blankStr(t) {
				out = append(out, t)
			}
		}
		return out
	}
	return []string{text(v)}
}

// IdeaText is the idea of an export as one line: a string as is; an object {name, problem, solution,
// audience} as "name: problem -> solution (audience: audience)".
func IdeaText(idea Value) string {
	if s, ok := idea.(string); ok {
		return s
	}
	if o, ok := idea.(*Object); ok {
		name, prob := text(o.GetOr("name", "")), text(o.GetOr("problem", ""))
		sol, aud := text(o.GetOr("solution", "")), text(o.GetOr("audience", ""))
		s := pyStrip(name+": "+prob, ": ")
		if sol != "" {
			s += " -> " + sol
		}
		if aud != "" {
			s += " (audience: " + aud + ")"
		}
		return s
	}
	return text(idea)
}

// Detect returns "smartsavant", "smartsignal-rounds", "smartsignal-result" or "".
func Detect(doc Value) string {
	o, ok := doc.(*Object)
	if !ok {
		return ""
	}
	if o.GetOr("schema", nil) == SchemaCouncil {
		return "smartsavant"
	}
	rounds, ok := o.GetOr("rounds", nil).([]Value)
	if !ok || len(rounds) == 0 {
		return ""
	}
	anyViews := false
	for _, r := range rounds {
		if ro, ok := r.(*Object); ok && ro.Has("views") {
			anyViews = true
			break
		}
	}
	if !anyViews {
		return ""
	}
	for _, r := range rounds {
		for _, v := range asList(asObject(r).GetOr("views", nil)) {
			if vo, ok := v.(*Object); ok {
				if vo.Has("parsed") {
					return "smartsignal-rounds"
				}
				return "smartsignal-result"
			}
		}
	}
	return "smartsignal-result"
}

func smallInt(v Value) (int, bool) {
	n, ok := v.(int64)
	if ok && n >= 0 && n < 1000 {
		return int(n), true
	}
	return 0, false
}

// FromSmartSignal imports a SmartSignal council export.
func FromSmartSignal(docValue Value) (*CouncilResult, error) {
	kind := Detect(docValue)
	if kind != "smartsignal-rounds" && kind != "smartsignal-result" {
		return nil, errors.New("not a SmartSignal council export (no rounds with views)")
	}
	doc := docValue.(*Object)
	models := map[string]bool{}
	res := &CouncilResult{Schema: SchemaCouncil, Idea: clean(IdeaText(doc.GetOr("idea", ""))), Synthetic: false,
		Stakeholders: []string{}, Sources: []string{}, Rounds: []CouncilRound{}, DecisionTree: []DecisionNode{},
		LockedDecisions: []string{}, Flags: []*Object{}}
	seen := map[string]int{}
	var names []string
	for _, rv := range doc.vals["rounds"].([]Value) {
		r, ok := rv.(*Object)
		if !ok {
			continue
		}
		step := text(r.GetOr("step_id", "step"))
		if step == "" {
			step = "step"
		}
		it := 1
		if n, ok := smallInt(r.GetOr("iteration", nil)); ok {
			it = n + 1
		} else if n, ok := smallInt(r.GetOr("round", nil)); ok {
			it = n + 1
		}
		key := Repr([]Value{step, int64(it)})
		seen[key]++
		if seen[key] > 1 {
			step = fmt.Sprintf("%s.%d", step, seen[key])
		}
		views := []StakeholderView{}
		for _, vv := range asList(r.GetOr("views", nil)) {
			v, ok := vv.(*Object)
			if !ok {
				continue
			}
			var p *Object
			if kind == "smartsignal-rounds" {
				p = asObject(v.GetOr("parsed", v))
			} else {
				p = v
			}
			src := text(v.GetOr("source", ""))
			if m := v.GetOr("model", nil); Truthy(m) {
				models[text(m)] = true
			}
			w := asObject(p.GetOr("w5h1", nil))
			who := text(v.GetOr("stakeholder", ""))
			if !contains(names, who) {
				names = append(names, who)
			}
			status := "ok"
			if src == "offline" {
				status = "offline"
			}
			var sources []string
			for _, s := range importStrs(p.GetOr("sources", nil)) {
				if Len(s) <= 200 {
					sources = append(sources, s)
				}
			}
			views = append(views, StakeholderView{Stakeholder: who, Role: text(v.GetOr("role", "")), Status: status,
				W5H1: W5H1{text(w.GetOr("who", "")), text(w.GetOr("what", "")), text(w.GetOr("where", "")),
					text(w.GetOr("when", "")), text(w.GetOr("why", "")), text(w.GetOr("how", ""))},
				DoneSoFar: text(p.GetOr("done_so_far", nil)), Missing: importStrs(p.GetOr("missing", nil)),
				DevilsAdvocate: text(p.GetOr("devils_advocate", nil)), Contribution: text(p.GetOr("contribution", nil)),
				Sources: copyStrs(sources), Error: ""})
		}
		j := asObject(r.GetOr("juror", nil))
		if m := j.GetOr("model", nil); Truthy(m) {
			models[text(m)] = true
		}
		st := text(j.GetOr("synthesis_status", ""))
		if st == "" || !contains(importStatus, st) {
			st = "failed"
		}
		resolved := importStrs(j.GetOr("resolved", nil))
		decision := text(j.GetOr("decision", nil))
		if decision == "" && len(resolved) > 0 {
			decision = resolved[0]
		}
		juror := JurorMerge{SynthesisStatus: st, MergeSummary: text(j.GetOr("merge_summary", nil)), Decision: decision,
			Resolved: resolved, OpenIssues: importStrs(j.GetOr("open_issues", nil)),
			NextSteps: importStrs(j.GetOr("next_steps", nil)), Reversals: importStrs(j.GetOr("reversals", nil)),
			SpecDelta: text(j.GetOr("spec_delta", nil)), LoopRecommended: j.GetOr("loop_recommended", nil) == true}
		if st == "full" && !SynthesisOK(juror.Value()) {
			juror.SynthesisStatus = "thin"
			juror.Error = "imported as full, but it fails the synthesis gate"
		}
		title := text(r.GetOr("step_title", step))
		if title == "" {
			title = step
		}
		question := text(r.GetOr("question", r.GetOr("step_title", "")))
		res.Rounds = append(res.Rounds, CouncilRound{StepID: step, StepTitle: title, Iteration: it, Question: question,
			Views: views, Juror: juror})
		branches := []string{}
		for _, x := range views {
			if x.Status == "ok" {
				branches = append(branches, x.Contribution)
			}
		}
		res.DecisionTree = append(res.DecisionTree, DecisionNode{StepID: step, Iteration: it, Branches: branches,
			Decision: juror.Decision, NextSteps: copyStrs(juror.NextSteps)})
		for _, x := range views {
			if x.Status != "ok" {
				continue
			}
			fv := Obj("stakeholder", x.Stakeholder, "contribution", x.Contribution, "devils_advocate", x.DevilsAdvocate,
				"done_so_far", x.DoneSoFar, "missing", strList(x.Missing), "sources", strList(x.Sources))
			for _, fl := range FlagUnsourced(fv, []Value{}) {
				res.Flags = append(res.Flags, fl.(*Object).Set("step_id", step).Set("iteration", int64(it)))
			}
		}
		if juror.SynthesisStatus == "full" {
			for _, d := range juror.Resolved {
				if !contains(res.LockedDecisions, d) {
					res.LockedDecisions = append(res.LockedDecisions, d)
				}
			}
		}
	}
	anyFull := false
	for _, r := range res.Rounds {
		if r.Juror.SynthesisStatus == "full" {
			anyFull = true
		}
	}
	if decs, ok := doc.GetOr("decisions", nil).([]Value); anyFull && ok && len(decs) > 0 {
		allObj := true
		for _, d := range decs {
			if _, isObj := d.(*Object); !isObj {
				allObj = false
			}
		}
		if allObj {
			locked := []string{}
			for _, d := range decs {
				do := d.(*Object)
				x := text(do.GetOr("topic", "")) + ": " + text(do.GetOr("decision", ""))
				if !IsFallback(x) {
					locked = append(locked, x)
				}
			}
			res.LockedDecisions = locked
		}
	}
	for _, s := range asList(doc.GetOr("stakeholders", nil)) {
		if so, ok := s.(*Object); ok {
			res.Stakeholders = append(res.Stakeholders, text(so.GetOr("name", "")))
		} else {
			res.Stakeholders = append(res.Stakeholders, text(s))
		}
	}
	if len(res.Stakeholders) == 0 {
		res.Stakeholders = copyStrs(names)
	}
	if len(models) > 0 {
		ms := make([]string, 0, len(models))
		for m := range models {
			ms = append(ms, m)
		}
		res.Adapter = "smartsignal:" + strings.Join(sortedStrings(ms), ",")
	} else {
		res.Adapter = "smartsignal"
	}
	return res, nil
}
