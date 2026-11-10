package smartsavant

// Audit a council run, its stories or its trainset before anyone builds or trains from it
// (spec/SEMANTICS.md section 7). The verdict is "build-ready" only when no finding has severity "error".
//
//	AUDIT-NO-SYNTHESIS          error    a phase's last merge is not a full synthesis
//	AUDIT-OFFLINE-VIEWS         error    views from an offline template (warning when fewer than half)
//	AUDIT-FAILED-VIEWS          warning  views the adapter never delivered validly
//	AUDIT-FALLBACK-TEXT         error    fallback text in summaries, decisions or spec deltas
//	AUDIT-TEMPLATE-VIEWS        error    the same view text repeated across 3 or more phases
//	AUDIT-GENERIC-STEPS         warning  next steps or story titles that name nothing (error when all are)
//	AUDIT-PLACEHOLDER-CODE      warning  "TODO" / return {'ok': True} code presented as content
//	AUDIT-UNSOURCED             warning  figures without a source, or sources the run did not provide
//	AUDIT-COMPLIANCE            error    a compliance rule fired
//	AUDIT-SYNTHETIC             error    a synthetic run (stub, demo)
//	AUDIT-TRAINSET-CONTAMINATED error    trainset rows that teach fallback or placeholder output
//	AUDIT-TRAINSET-UNREADABLE   error    trainset rows that are not objects with an output or messages

import (
	"errors"
	"fmt"
	"io"
	"os"
	"sort"
	"strings"
)

// MaxExamples is the number of examples kept per finding.
const MaxExamples = 5

// Finding is one audit finding.
type Finding struct {
	Code     string
	Severity string
	Count    int
	Detail   string
	Examples []string // at most 5, each at most 200 code points
}

// Report is an audit report.
type Report struct {
	Kind     string // council, stories or trainset
	Summary  *Object
	Findings []Finding
	Verdict  string // build-ready or not build-ready
	Source   string // for an imported council: the detected shape (not part of Value when empty)
}

// Value is the JSON form.
func (r Report) Value() *Object {
	fs := make([]Value, len(r.Findings))
	for i, f := range r.Findings {
		fs[i] = Obj("code", f.Code, "severity", f.Severity, "count", int64(f.Count), "detail", f.Detail,
			"examples", strList(f.Examples))
	}
	o := Obj("kind", r.Kind, "summary", r.Summary, "findings", fs, "verdict", r.Verdict)
	if r.Source != "" {
		o.Set("source", r.Source)
	}
	return o
}

func finding(code, severity string, count int, detail string, examples []string) Finding {
	ex := []string{}
	for i, e := range examples {
		if i == MaxExamples {
			break
		}
		ex = append(ex, Head(e, 200))
	}
	return Finding{code, severity, count, detail, ex}
}

var severityOrder = map[string]int{"error": 0, "warning": 1, "info": 2}

func report(kind string, summary *Object, findings []Finding) Report {
	sort.SliceStable(findings, func(i, j int) bool {
		a, b := findings[i], findings[j]
		if severityOrder[a.Severity] != severityOrder[b.Severity] {
			return severityOrder[a.Severity] < severityOrder[b.Severity]
		}
		return a.Code < b.Code
	})
	verdict := "build-ready"
	for _, f := range findings {
		if f.Severity == "error" {
			verdict = "not build-ready"
		}
	}
	if findings == nil {
		findings = []Finding{}
	}
	return Report{Kind: kind, Summary: summary, Findings: findings, Verdict: verdict}
}

func decided(r CouncilRound) bool {
	return r.Juror.SynthesisStatus == "full" && SynthesisOK(r.Juror.Value())
}

// AuditResult audits a council result.
func AuditResult(res *CouncilResult) Report {
	h := HealthOf(res)
	var out []Finding
	last := lastRounds(res)
	var undecided []string
	for _, r := range last {
		if !decided(r) {
			s := r.StepID + ": " + r.Juror.SynthesisStatus
			if r.Juror.SynthesisStatus == "full" {
				s += " (recorded, but it fails the synthesis gate)"
			}
			undecided = append(undecided, s)
		}
	}
	if len(undecided) > 0 {
		out = append(out, finding("AUDIT-NO-SYNTHESIS", "error", len(undecided),
			fmt.Sprintf("%d of %d phases have no full synthesis", len(undecided), len(last)), undecided))
	}
	nViews := 0
	var off, failed, ph []string
	for _, r := range res.Rounds {
		for _, v := range r.Views {
			nViews++
			if v.Status == "offline" {
				off = append(off, r.StepID+"/"+v.Stakeholder)
			}
			if v.Status == "failed" {
				failed = append(failed, r.StepID+"/"+v.Stakeholder+": "+v.Error)
			}
		}
	}
	if len(off) > 0 {
		sev := "warning"
		if 2*len(off) >= max(1, nViews) {
			sev = "error"
		}
		out = append(out, finding("AUDIT-OFFLINE-VIEWS", sev, len(off),
			fmt.Sprintf("%d of %d views are offline templates", len(off), nViews), off))
	}
	if len(failed) > 0 {
		out = append(out, finding("AUDIT-FAILED-VIEWS", "warning", len(failed),
			fmt.Sprintf("%d views were never valid", len(failed)), failed))
	}
	var fb []string
	for _, r := range res.Rounds {
		j := r.Juror
		type nt struct{ name, t string }
		fields := []nt{{"merge_summary", j.MergeSummary}, {"decision", j.Decision}, {"spec_delta", j.SpecDelta}}
		for _, k := range []struct {
			name string
			xs   []string
		}{{"resolved", j.Resolved}, {"next_steps", j.NextSteps}, {"open_issues", j.OpenIssues}} {
			for i, t := range k.xs {
				fields = append(fields, nt{fmt.Sprintf("%s[%d]", k.name, i), t})
			}
		}
		for _, f := range fields {
			if IsFallback(f.t) {
				fb = append(fb, r.StepID+"."+f.name+": "+Head(f.t, 80))
			}
		}
	}
	if len(fb) > 0 {
		out = append(out, finding("AUDIT-FALLBACK-TEXT", "error", len(fb), "fallback text where a decision should be", fb))
	}
	// the same devil's-advocate or contribution text in several phases: a template
	type tkey struct{ who, text string }
	var order []tkey
	steps := map[tkey]map[string]bool{}
	for _, r := range res.Rounds {
		for _, v := range r.Views {
			for _, t := range []string{v.DevilsAdvocate, v.Contribution} {
				k := tkey{v.Stakeholder, Squash(t)}
				if Len(k.text) >= 20 {
					if steps[k] == nil {
						steps[k] = map[string]bool{}
						order = append(order, k)
					}
					steps[k][strings.SplitN(r.StepID, ".", 2)[0]] = true
				}
			}
		}
	}
	var tmpl []string
	for _, k := range order {
		if len(steps[k]) >= 3 {
			tmpl = append(tmpl, k.who+": "+Head(k.text, 60))
		}
	}
	if len(tmpl) > 0 {
		out = append(out, finding("AUDIT-TEMPLATE-VIEWS", "error", len(tmpl), "the same view text appears in 3 or more phases", tmpl))
	}
	var allSteps, gen []string
	for _, r := range last {
		for _, s := range r.Juror.NextSteps {
			allSteps = append(allSteps, s)
			if IsGeneric(s) {
				gen = append(gen, s)
			}
		}
	}
	if len(gen) > 0 {
		sev := "warning"
		if len(gen) == len(allSteps) {
			sev = "error"
		}
		out = append(out, finding("AUDIT-GENERIC-STEPS", sev, len(gen),
			fmt.Sprintf("%d of %d next steps name nothing", len(gen), len(allSteps)), gen))
	}
	for _, r := range res.Rounds {
		for _, v := range r.Views {
			if HasPlaceholderCode(v.Contribution) {
				ph = append(ph, r.StepID+"/"+v.Stakeholder)
			}
		}
	}
	for _, r := range res.Rounds {
		if HasPlaceholderCode(r.Juror.SpecDelta) {
			ph = append(ph, r.StepID+".spec_delta")
		}
	}
	if len(ph) > 0 {
		out = append(out, finding("AUDIT-PLACEHOLDER-CODE", "warning", len(ph), "placeholder code presented as content", ph))
	}
	if len(res.Flags) > 0 {
		var ex []string
		for _, x := range res.Flags {
			what := x.GetOr("figure", nil)
			if !Truthy(what) {
				what = x.GetOr("source", nil)
			}
			ex = append(ex, Str(x.GetOr("step_id", nil))+"/"+Str(x.GetOr("stakeholder", nil))+": "+Str(what))
		}
		out = append(out, finding("AUDIT-UNSOURCED", "warning", len(res.Flags), "figures without a source, or unknown sources", ex))
	}
	texts := []string{res.Idea}
	for _, r := range res.Rounds {
		texts = append(texts, r.Juror.MergeSummary, r.Juror.Decision, r.Juror.SpecDelta)
		for _, v := range r.Views {
			texts = append(texts, v.Contribution)
		}
	}
	if comp := ComplianceBlock(strings.Join(texts, " "), nil); len(comp) > 0 {
		out = append(out, finding("AUDIT-COMPLIANCE", "error", len(comp), "compliance rules fired", comp))
	}
	if res.Synthetic || res.Adapter == "stub" || res.Adapter == "scripted" || strings.HasPrefix(res.Adapter, "demo") {
		out = append(out, finding("AUDIT-SYNTHETIC", "error", 1,
			fmt.Sprintf("adapter %s is synthetic: not a real deliberation", res.Adapter), nil))
	}
	return report("council", h.Value(), out)
}

func strsOf(vs []Value) []string {
	out := make([]string, len(vs))
	for i, v := range vs {
		out[i] = Str(v)
	}
	return out
}

// AuditStories audits a stories document ({"stories": [...]}).
func AuditStories(doc Value) Report {
	var stories []Value
	if o, ok := doc.(*Object); ok {
		stories = pyIter(o.GetOr("stories", []Value{}))
	}
	var titles, gen, blocked []Value
	for _, s := range stories {
		if so, ok := s.(*Object); ok {
			t := so.GetOr("title", "")
			titles = append(titles, t)
			if IsGeneric(t) {
				gen = append(gen, t)
			}
			if so.GetOr("status", nil) == "blocked" {
				blocked = append(blocked, so.GetOr("story_id", ""))
			}
		}
	}
	var out []Finding
	if len(gen) > 0 {
		sev := "warning"
		if len(gen) == len(titles) {
			sev = "error"
		}
		out = append(out, finding("AUDIT-GENERIC-STEPS", sev, len(gen),
			fmt.Sprintf("%d of %d story titles name nothing", len(gen), len(titles)), strsOf(gen)))
	}
	if len(blocked) > 0 {
		out = append(out, finding("AUDIT-NO-SYNTHESIS", "error", len(blocked),
			fmt.Sprintf("%d stories are blocked (no decision)", len(blocked)), strsOf(blocked)))
	}
	if len(titles) == 0 {
		out = append(out, finding("AUDIT-NO-SYNTHESIS", "error", 0, "no stories", nil))
	}
	return report("stories", Obj("stories", int64(len(titles))), out)
}

func rowText(row *Object) string {
	if msgs, ok := row.GetOr("messages", nil).([]Value); ok {
		var parts []string
		for _, m := range msgs {
			if mo, ok := m.(*Object); ok && mo.GetOr("role", nil) == "assistant" {
				parts = append(parts, Str(mo.GetOr("content", "")))
			}
		}
		return strings.Join(parts, " ")
	}
	return Str(row.GetOr("output", ""))
}

// AuditTrainset audits trainset rows (parsed JSON lines; nil for a line that is not JSON).
func AuditTrainset(rows []Value) Report {
	var bad, unreadable []string
	kinds := map[string]int{}
	for i, rv := range rows {
		row, ok := rv.(*Object)
		_, outStr := row.GetOr("output", nil).(string)
		_, msgList := row.GetOr("messages", nil).([]Value)
		if !ok || !(outStr || msgList) {
			unreadable = append(unreadable, fmt.Sprintf("row %d: not an object with an output or messages", i))
			continue
		}
		k := "unknown"
		if meta, ok := row.GetOr("meta", NewObject()).(*Object); ok {
			if s, ok := meta.GetOr("kind", nil).(string); ok {
				k = s
			}
		}
		kinds[k]++
		t := rowText(row)
		var why []string
		if IsFallback(t) {
			why = append(why, "fallback")
		}
		if HasPlaceholderCode(t) {
			why = append(why, "placeholder code")
		}
		if len(why) > 0 {
			bad = append(bad, fmt.Sprintf("row %d (%s): %s", i, k, strings.Join(why, ", ")))
		}
	}
	var out []Finding
	if len(unreadable) > 0 {
		out = append(out, finding("AUDIT-TRAINSET-UNREADABLE", "error", len(unreadable),
			fmt.Sprintf("%d of %d rows cannot be used", len(unreadable), len(rows)), unreadable))
	}
	if len(bad) > 0 {
		out = append(out, finding("AUDIT-TRAINSET-CONTAMINATED", "error", len(bad),
			fmt.Sprintf("%d of %d rows teach fallback or placeholder output", len(bad), len(rows)), bad))
	}
	if len(rows) == 0 {
		out = append(out, finding("AUDIT-TRAINSET-CONTAMINATED", "error", 0, "no rows", nil))
	}
	kl := []Value{}
	for _, c := range countSorted(kinds) {
		kl = append(kl, []Value{c.Key, int64(c.N)})
	}
	return report("trainset", Obj("rows", int64(len(rows)), "kinds", kl), out)
}

// Budgets of ReadJSONL, as in the reference.
const (
	MaxRows  = 200000
	MaxChars = 256 * 1024 * 1024 // code points of all lines read
	MaxLine  = 1024 * 1024       // longer lines count as unreadable rows
	MaxLines = 1000000           // lines read, empty ones included
)

// ReadJSONL parses JSON lines (nil for a line that is not JSON), stopping at the row, size and line
// budgets. Invalid UTF-8 is read as U+FFFD; "\r\n" and "\r" end lines as "\n" does (Python's universal
// newlines).
func ReadJSONL(r io.Reader) ([]Value, error) {
	data, err := io.ReadAll(r)
	if err != nil {
		return nil, err
	}
	text := strings.ToValidUTF8(string(data), "\uFFFD")
	text = strings.ReplaceAll(strings.ReplaceAll(text, "\r\n", "\n"), "\r", "\n")
	rows := []Value{}
	budget := [3]int{MaxRows, MaxChars, MaxLines}
	for _, line := range strings.SplitAfter(text, "\n") {
		if line == "" {
			continue
		}
		budget[1] -= Len(line)
		budget[2]--
		if budget[1] < 0 || budget[0] <= 0 || budget[2] < 0 {
			break
		}
		line = strings.Trim(line, " \t\r\n")
		if line == "" {
			continue
		}
		budget[0]--
		if Len(line) > MaxLine {
			rows = append(rows, nil)
		} else if v, perr := ParseJSON([]byte(line)); perr == nil {
			rows = append(rows, v)
		} else {
			rows = append(rows, nil)
		}
	}
	return rows, nil
}

// AuditPath audits one file: a council JSON (this package or SmartSignal), a stories JSON, or a .jsonl
// trainset. Zip archives are read by the Python reference only.
func AuditPath(path string) (Report, error) {
	if strings.HasSuffix(path, ".zip") {
		return Report{}, errors.New("zip trainsets are read by the Python reference only")
	}
	f, err := os.Open(path)
	if err != nil {
		return Report{}, err
	}
	defer f.Close()
	if strings.HasSuffix(path, ".jsonl") {
		rows, err := ReadJSONL(f)
		if err != nil {
			return Report{}, err
		}
		return AuditTrainset(rows), nil
	}
	data, err := io.ReadAll(f)
	if err != nil {
		return Report{}, err
	}
	doc, err := ParseJSON(data)
	if err != nil {
		return Report{}, err
	}
	if l, ok := doc.([]Value); ok && len(l) > 0 {
		all := true
		for _, r := range l {
			if ro, ok := r.(*Object); !ok || !ro.Has("views") {
				all = false
			}
		}
		if all {
			doc = Obj("rounds", l) // a bare list of rounds
		}
	}
	switch kind := Detect(doc); kind {
	case "smartsavant":
		if errs := CheckCouncil(doc); len(errs) > 0 {
			if len(errs) > 3 {
				errs = errs[:3]
			}
			return Report{}, errors.New("not a valid council file: " + strings.Join(errs, "; "))
		}
		return AuditResult(ResultFromValue(doc.(*Object))), nil
	case "":
	default:
		res, err := FromSmartSignal(doc)
		if err != nil {
			return Report{}, err
		}
		rep := AuditResult(res)
		rep.Source = kind
		return rep, nil
	}
	if o, ok := doc.(*Object); ok {
		if _, isList := o.GetOr("stories", nil).([]Value); isList {
			return AuditStories(doc), nil
		}
	}
	return Report{}, errors.New("not a council result, a council export, a stories file or a trainset")
}
