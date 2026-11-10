package smartsavant

// The council data model (bindings/council.d.ts, src/sf_smartsavant/model.py). Value() gives the JSON
// form with the fields in the order of the Python dataclasses.
//
//	view.status              ok | failed | offline
//	juror.synthesis_status   full | thin | recovered | failed | offline | no_views

// SchemaCouncil is the schema id of a council result.
const SchemaCouncil = "smartsavant.council/1"

// W5H1 is who, what, where, when, why, how.
type W5H1 struct {
	Who, What, Where, When, Why, How string
}

// StakeholderView is one stakeholder's view in one round.
type StakeholderView struct {
	Stakeholder    string
	Role           string
	Status         string
	W5H1           W5H1
	DoneSoFar      string
	Missing        []string
	DevilsAdvocate string
	Contribution   string
	Sources        []string
	Error          string
}

// JurorMerge is the juror's merge of one round.
type JurorMerge struct {
	SynthesisStatus string
	MergeSummary    string
	Decision        string
	Resolved        []string
	OpenIssues      []string
	NextSteps       []string
	Reversals       []string
	SpecDelta       string
	LoopRecommended bool
	Error           string
}

// CouncilRound is one iteration of one phase.
type CouncilRound struct {
	StepID    string
	StepTitle string
	Iteration int
	Question  string
	Views     []StakeholderView
	Juror     JurorMerge
}

// DecisionNode is the decision-tree node of one round.
type DecisionNode struct {
	StepID    string
	Iteration int
	Branches  []string
	Decision  string
	NextSteps []string
}

// CouncilResult is the result of a run (or of an import).
type CouncilResult struct {
	Schema          string
	Idea            string
	Adapter         string
	Synthetic       bool
	Stakeholders    []string
	Sources         []string
	Rounds          []CouncilRound
	DecisionTree    []DecisionNode
	LockedDecisions []string
	// Flags are objects {flag, figure | source, stakeholder, step_id, iteration}.
	Flags []*Object
}

func strList(xs []string) []Value {
	out := make([]Value, len(xs))
	for i, x := range xs {
		out[i] = x
	}
	return out
}

func copyStrs(xs []string) []string { return append([]string{}, xs...) }

// Value is the JSON form.
func (w W5H1) Value() *Object {
	return Obj("who", w.Who, "what", w.What, "where", w.Where, "when", w.When, "why", w.Why, "how", w.How)
}

// Value is the JSON form.
func (v StakeholderView) Value() *Object {
	return Obj("stakeholder", v.Stakeholder, "role", v.Role, "status", v.Status, "w5h1", v.W5H1.Value(),
		"done_so_far", v.DoneSoFar, "missing", strList(v.Missing), "devils_advocate", v.DevilsAdvocate,
		"contribution", v.Contribution, "sources", strList(v.Sources), "error", v.Error)
}

// Value is the JSON form.
func (j JurorMerge) Value() *Object {
	return Obj("synthesis_status", j.SynthesisStatus, "merge_summary", j.MergeSummary, "decision", j.Decision,
		"resolved", strList(j.Resolved), "open_issues", strList(j.OpenIssues), "next_steps", strList(j.NextSteps),
		"reversals", strList(j.Reversals), "spec_delta", j.SpecDelta, "loop_recommended", j.LoopRecommended,
		"error", j.Error)
}

// Value is the JSON form.
func (r CouncilRound) Value() *Object {
	views := make([]Value, len(r.Views))
	for i, v := range r.Views {
		views[i] = v.Value()
	}
	return Obj("step_id", r.StepID, "step_title", r.StepTitle, "iteration", int64(r.Iteration),
		"question", r.Question, "views", views, "juror", r.Juror.Value())
}

// Value is the JSON form.
func (n DecisionNode) Value() *Object {
	return Obj("step_id", n.StepID, "iteration", int64(n.Iteration), "branches", strList(n.Branches),
		"decision", n.Decision, "next_steps", strList(n.NextSteps))
}

// Value is the JSON form (council.json).
func (c *CouncilResult) Value() *Object {
	rounds := make([]Value, len(c.Rounds))
	for i, r := range c.Rounds {
		rounds[i] = r.Value()
	}
	tree := make([]Value, len(c.DecisionTree))
	for i, n := range c.DecisionTree {
		tree[i] = n.Value()
	}
	flags := make([]Value, len(c.Flags))
	for i, f := range c.Flags {
		flags[i] = f
	}
	schema := c.Schema
	if schema == "" {
		schema = SchemaCouncil
	}
	return Obj("schema", schema, "idea", c.Idea, "adapter", c.Adapter, "synthetic", c.Synthetic,
		"stakeholders", strList(c.Stakeholders), "sources", strList(c.Sources), "rounds", rounds,
		"decision_tree", tree, "locked_decisions", strList(c.LockedDecisions), "flags", flags)
}

func valueStrs(v Value) []string {
	out := []string{}
	for _, x := range pyIter(v) {
		if s, ok := x.(string); ok {
			out = append(out, s)
		}
	}
	return out
}

func valueStr(o *Object, k string) string { s, _ := o.GetOr(k, "").(string); return s }

func valueInt(o *Object, k string) int { n, _ := o.GetOr(k, int64(0)).(int64); return int(n) }

// ResultFromValue is the inverse of Value, for a council.json that passed CheckCouncil.
func ResultFromValue(d *Object) *CouncilResult {
	res := &CouncilResult{Schema: valueStr(d, "schema"), Idea: valueStr(d, "idea"), Adapter: valueStr(d, "adapter"),
		Stakeholders: valueStrs(d.GetOr("stakeholders", nil)), Sources: valueStrs(d.GetOr("sources", nil)),
		LockedDecisions: valueStrs(d.GetOr("locked_decisions", nil)), Rounds: []CouncilRound{},
		DecisionTree: []DecisionNode{}, Flags: []*Object{}}
	res.Synthetic, _ = d.GetOr("synthetic", false).(bool)
	for _, rv := range pyIter(d.GetOr("rounds", nil)) {
		r, _ := rv.(*Object)
		j, _ := r.GetOr("juror", nil).(*Object)
		rnd := CouncilRound{StepID: valueStr(r, "step_id"), StepTitle: valueStr(r, "step_title"),
			Iteration: valueInt(r, "iteration"), Question: valueStr(r, "question"), Views: []StakeholderView{}}
		for _, vv := range pyIter(r.GetOr("views", nil)) {
			v, _ := vv.(*Object)
			w, _ := v.GetOr("w5h1", nil).(*Object)
			rnd.Views = append(rnd.Views, StakeholderView{Stakeholder: valueStr(v, "stakeholder"), Role: valueStr(v, "role"),
				Status: valueStr(v, "status"), W5H1: W5H1{valueStr(w, "who"), valueStr(w, "what"), valueStr(w, "where"),
					valueStr(w, "when"), valueStr(w, "why"), valueStr(w, "how")},
				DoneSoFar: valueStr(v, "done_so_far"), Missing: valueStrs(v.GetOr("missing", nil)),
				DevilsAdvocate: valueStr(v, "devils_advocate"), Contribution: valueStr(v, "contribution"),
				Sources: valueStrs(v.GetOr("sources", nil)), Error: valueStr(v, "error")})
		}
		loop, _ := j.GetOr("loop_recommended", false).(bool)
		rnd.Juror = JurorMerge{SynthesisStatus: valueStr(j, "synthesis_status"), MergeSummary: valueStr(j, "merge_summary"),
			Decision: valueStr(j, "decision"), Resolved: valueStrs(j.GetOr("resolved", nil)),
			OpenIssues: valueStrs(j.GetOr("open_issues", nil)), NextSteps: valueStrs(j.GetOr("next_steps", nil)),
			Reversals: valueStrs(j.GetOr("reversals", nil)), SpecDelta: valueStr(j, "spec_delta"),
			LoopRecommended: loop, Error: valueStr(j, "error")}
		res.Rounds = append(res.Rounds, rnd)
	}
	for _, nv := range pyIter(d.GetOr("decision_tree", nil)) {
		n, _ := nv.(*Object)
		res.DecisionTree = append(res.DecisionTree, DecisionNode{StepID: valueStr(n, "step_id"),
			Iteration: valueInt(n, "iteration"), Branches: valueStrs(n.GetOr("branches", nil)),
			Decision: valueStr(n, "decision"), NextSteps: valueStrs(n.GetOr("next_steps", nil))})
	}
	for _, f := range pyIter(d.GetOr("flags", nil)) {
		if fo, ok := f.(*Object); ok {
			res.Flags = append(res.Flags, fo)
		}
	}
	return res
}
