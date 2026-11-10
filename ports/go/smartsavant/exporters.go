package smartsavant

// Exporters (spec/SEMANTICS.md section 6): a council result -> run digest, user stories, a spec in
// Markdown, a fine-tune trainset, a Mermaid tree. Every exporter reports what the run did not decide.

import (
	"fmt"
	"strings"
)

// RunDigest is the lowercase hex SHA-256 of the canonical JSON of the whole result.
func RunDigest(res *CouncilResult) string {
	t, err := Canonical(res.Value())
	if err != nil { // a result holds only strings, integers, booleans, lists and objects
		panic(err)
	}
	return SHA256Hex(t)
}

// ToStories returns {schema, project_key, run, counts, stories}: one story per phase, from its last
// iteration; ready when that merge is full and has a non-generic next step, blocked otherwise.
func ToStories(res *CouncilResult, projectKey string) *Object {
	stories := []Value{}
	ready, blocked := 0, 0
	for i, r := range lastRounds(res) {
		j := r.Juror
		sid := fmt.Sprintf("STORY-%s-%04d", projectKey, i+1)
		var steps []string
		for _, s := range j.NextSteps {
			if !IsGeneric(s) {
				steps = append(steps, s)
			}
		}
		if j.SynthesisStatus == "full" && len(steps) > 0 {
			ac := []Value{"Implements the decision: " + j.Decision}
			for _, x := range j.Resolved {
				ac = append(ac, "Holds: "+x)
			}
			ac = append(ac, "A test exercises it end to end.")
			stories = append(stories, Obj("story_id", sid, "status", "ready", "epic", r.StepTitle, "title", steps[0],
				"decision", j.Decision, "sub_tasks", strList(steps), "acceptance_criteria", ac,
				"open_issues", strList(j.OpenIssues), "phase", r.StepID, "iteration", int64(r.Iteration)))
			ready++
		} else {
			issues := strList(j.OpenIssues)
			if j.Error != "" {
				issues = append(issues, j.Error)
			}
			stories = append(stories, Obj("story_id", sid, "status", "blocked", "epic", r.StepTitle,
				"title", fmt.Sprintf("Decide %s: the council did not reach a decision (%s)", r.StepTitle, j.SynthesisStatus),
				"decision", "", "sub_tasks", []Value{}, "acceptance_criteria", []Value{}, "open_issues", issues,
				"phase", r.StepID, "iteration", int64(r.Iteration)))
			blocked++
		}
	}
	return Obj("schema", "smartsavant.stories/1", "project_key", projectKey, "run", RunDigest(res),
		"counts", Obj("stories", int64(len(stories)), "ready", int64(ready), "blocked", int64(blocked)),
		"stories", stories)
}

// badChar: control characters and bidirectional controls, shown as U+FFFD.
func badChar(r rune) bool {
	return r < 32 || (r >= 127 && r < 160) || (r >= 0x202A && r <= 0x202E) || (r >= 0x2066 && r <= 0x2069)
}

// Md makes model text inert in Markdown: one line (Squash), control and bidirectional-control characters
// as U+FFFD, & < > as HTML entities, and every Markdown metacharacter \ ` * _ { } [ ] ( ) # + - . ! | ~
// backslash-escaped.
func Md(text string) string {
	var b strings.Builder
	for _, c := range Squash(text) {
		switch {
		case badChar(c):
			b.WriteRune('�')
		case c == '&':
			b.WriteString("&amp;")
		case c == '<':
			b.WriteString("&lt;")
		case c == '>':
			b.WriteString("&gt;")
		case strings.ContainsRune("\\`*_{}[]()#+-.!|~", c):
			b.WriteByte('\\')
			b.WriteRune(c)
		default:
			b.WriteRune(c)
		}
	}
	return b.String()
}

// ToSpecMd returns the spec in Markdown; undecided phases are marked NOT DECIDED.
func ToSpecMd(res *CouncilResult) string {
	h := HealthOf(res)
	hv := h.Value()
	synth := ""
	if res.Synthetic {
		synth = " (synthetic: not a real deliberation)"
	}
	lines := []string{"# Council spec", "", "*" + Md(res.Idea) + "*", "",
		fmt.Sprintf("Run %s with adapter %s%s. %d of %d phases decided; views %s; merges %s; %d flag(s).",
			RunDigest(res)[:16], Md(res.Adapter), synth, h.PhasesDecided, h.Phases,
			Repr(hv.vals["views"]), Repr(hv.vals["merges"]), h.Flags), ""}
	if !h.Decided {
		lines = append(lines, "> **Not build-ready:** at least one phase has no full synthesis. Do not build from the "+
			"undecided phases; re-run them with a working adapter.", "")
	}
	if len(res.LockedDecisions) > 0 {
		lines = append(lines, "## Decisions", "", "| # | Decision |", "|---|---|")
		for i, d := range res.LockedDecisions {
			lines = append(lines, fmt.Sprintf("| %d | %s |", i+1, Md(d)))
		}
		lines = append(lines, "")
	}
	for _, r := range lastRounds(res) {
		j := r.Juror
		lines = append(lines, "## "+Md(r.StepTitle), "", "**Question:** "+Md(r.Question), "")
		if j.SynthesisStatus == "full" {
			lines = append(lines, "**Decision:** "+Md(j.Decision), "", Md(j.MergeSummary), "")
			if len(j.NextSteps) > 0 {
				lines = append(lines, "**Next steps:**", "")
				for _, s := range j.NextSteps {
					lines = append(lines, "- "+Md(s))
				}
				lines = append(lines, "")
			}
		} else {
			e := ""
			if j.Error != "" {
				e = ": " + Md(j.Error)
			}
			lines = append(lines, fmt.Sprintf("**NOT DECIDED** (%s)%s", j.SynthesisStatus, e), "")
		}
		if len(j.OpenIssues) > 0 {
			lines = append(lines, "**Open issues:**", "")
			for _, s := range j.OpenIssues {
				lines = append(lines, "- "+Md(s))
			}
			lines = append(lines, "")
		}
		var failed []string
		for _, v := range r.Views {
			if v.Status != "ok" {
				failed = append(failed, v.Stakeholder)
			}
		}
		if len(failed) > 0 {
			lines = append(lines, "*No valid view from: "+Md(strings.Join(failed, ", "))+".*", "")
		}
	}
	if len(res.Flags) > 0 {
		lines = append(lines, "## Flags", "", fmt.Sprintf("%d finding(s) to review before use:", len(res.Flags)), "")
		for i, f := range res.Flags {
			if i == 200 {
				break
			}
			what := f.GetOr("figure", nil)
			if !Truthy(what) {
				what = f.GetOr("source", nil)
			}
			if !Truthy(what) {
				what = ""
			}
			lines = append(lines, fmt.Sprintf("- %s: %s (%s, %s)", Str(f.GetOr("flag", nil)), Md(Str(what)),
				Md(Str(f.GetOr("stakeholder", ""))), Md(Str(f.GetOr("step_id", "")))))
		}
		lines = append(lines, "")
	}
	return strings.Join(lines, "\n")
}

// Methodology is the facilitation guidance per default phase; used only when the round's title starts
// with the phase id.
var Methodology = map[string]string{
	"frame":     "Establish the problem, the real users and their jobs-to-be-done, and agree on one long-term goal.",
	"concept":   "Collect solution directions, then pick the wedge: the smallest offer the users would switch for.",
	"design":    "Draw the user flow and the data flow end to end, and name every screen and every store.",
	"build":     "Decide the technical approach and the riskiest part to prove first.",
	"integrate": "Fit the design into the target architecture: owners, interfaces, auth, deployment.",
	"measure":   "Set the KPIs, how each one is measured, and the thresholds that would change the plan.",
	"spec":      "Assemble the decisions into one document that an engineer can build from.",
}

// ToTrainset returns the rows {instruction, input, output, meta}: a methodology row per default phase
// used; expert rows (ok views without a flag) and facilitation rows (full merges) unless the run is
// synthetic and allowSynthetic is false.
func ToTrainset(res *CouncilResult, allowSynthetic bool) []Value {
	digest := RunDigest(res)
	rows := []Value{}
	flagged := map[string]bool{}
	for _, f := range res.Flags {
		flagged[Repr([]Value{f.GetOr("step_id", nil), f.GetOr("iteration", nil), f.GetOr("stakeholder", nil)})] = true
	}
	learn := allowSynthetic || !res.Synthetic
	seen := map[string]bool{}
	for _, r := range res.Rounds {
		meta := func(kv ...any) *Object {
			o := Obj("phase", r.StepID, "iteration", int64(r.Iteration), "run", digest, "adapter", res.Adapter)
			for i := 0; i+1 < len(kv); i += 2 {
				o.Set(kv[i].(string), kv[i+1])
			}
			return o
		}
		if m, ok := Methodology[r.StepID]; ok && !seen[r.StepID] && strings.HasPrefix(pyLower(r.StepTitle), r.StepID) {
			seen[r.StepID] = true
			rows = append(rows, Obj("instruction", fmt.Sprintf("We are at the %s phase of a design council. How should I facilitate it?", r.StepTitle),
				"input", "", "output", m, "meta", meta("kind", "methodology")))
		}
		if !learn {
			continue
		}
		for _, v := range r.Views {
			if v.Status == "ok" && !flagged[Repr([]Value{r.StepID, int64(r.Iteration), v.Stakeholder})] {
				vv := v.Value()
				out := NewObject()
				for _, k := range viewKeys {
					out.Set(k, vv.vals[k])
				}
				rows = append(rows, Obj("instruction", fmt.Sprintf("As the %s on a design council, give your view on: %s", v.Stakeholder, r.Question),
					"input", res.Idea, "output", PyDumps(out, true),
					"meta", meta("kind", "expert", "stakeholder", v.Stakeholder)))
			}
		}
		if r.Juror.SynthesisStatus == "full" {
			j := r.Juror
			var contribs []string
			for _, v := range r.Views {
				if v.Status == "ok" {
					contribs = append(contribs, v.Contribution)
				}
			}
			out := Obj("decision", j.Decision, "merge_summary", j.MergeSummary, "resolved", strList(j.Resolved),
				"next_steps", strList(j.NextSteps))
			rows = append(rows, Obj("instruction", fmt.Sprintf("Merge the panel for the %s phase as the juror.", r.StepTitle),
				"input", PyDumps(strList(contribs), false), "output", PyDumps(out, true),
				"meta", meta("kind", "facilitation", "synthesis", "full")))
		}
	}
	return rows
}

// MermaidLabel is a Mermaid label: squashed, control characters as U+FFFD, at most 80 code points, & " < > replaced, quoted.
func MermaidLabel(t string) string {
	var b strings.Builder
	for _, c := range Squash(t) {
		if badChar(c) {
			c = '�'
		}
		b.WriteRune(c)
	}
	s := Head(b.String(), 80)
	s = strings.ReplaceAll(s, "&", "&amp;")
	s = strings.ReplaceAll(s, `"`, "'")
	s = strings.ReplaceAll(s, "<", "&lt;")
	s = strings.ReplaceAll(s, ">", "&gt;")
	return `"` + s + `"`
}

// ToMermaid returns the decision tree as a Mermaid flowchart.
func ToMermaid(res *CouncilResult) string {
	lines := []string{"flowchart TD", "  start([Idea])"}
	prev := "start"
	for i, n := range res.DecisionTree {
		nid := fmt.Sprintf("n%d", i)
		r := res.Rounds[i]
		label := n.Decision
		if r.Juror.SynthesisStatus != "full" {
			label = "NOT DECIDED: " + r.Juror.SynthesisStatus
		}
		lines = append(lines, fmt.Sprintf("  %s{%s}", nid, MermaidLabel(fmt.Sprintf("%s #%d", r.StepTitle, n.Iteration))),
			fmt.Sprintf("  %s --> %s", prev, nid), fmt.Sprintf("  %sd[%s]", nid, MermaidLabel(label)),
			fmt.Sprintf("  %s --> %sd", nid, nid))
		prev = nid
	}
	return strings.Join(lines, "\n") + "\n"
}
