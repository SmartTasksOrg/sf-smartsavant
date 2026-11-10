package smartsavant

// The vector runner (spec/PORTING.md): the same checks as src/sf_smartsavant/conformance.py, one check
// per case and, for runs.json, one check per expected field.

import (
	"fmt"
	"os"
	"path/filepath"
)

func loadVectors(dir, name string) (*Object, error) {
	data, err := os.ReadFile(filepath.Join(dir, name))
	if err != nil {
		return nil, err
	}
	v, err := ParseJSON(data)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", name, err)
	}
	o, ok := v.(*Object)
	if !ok {
		return nil, fmt.Errorf("%s: not an object", name)
	}
	return o, nil
}

func show(v Value) string { return Head(PyDumps(v, true), 300) }

func cases(o *Object, k string) []Value { l, _ := o.GetOr(k, nil).([]Value); return l }

func vfield(c Value, k string) Value { o, _ := c.(*Object); return o.GetOr(k, nil) }

func fieldStr(c Value, k string) string { s, _ := vfield(c, k).(string); return s }

func strValues(xs []string) []Value { return strsToValues(xs) }

// RunConformance runs every vector file in dir and returns the number of checks and the failures.
func RunConformance(dir string) (total int, fails []string, err error) {
	check := func(name string, got func() (Value, error), want Value) {
		total++
		defer func() {
			if p := recover(); p != nil {
				fails = append(fails, fmt.Sprintf("%s: panic: %v", name, p))
			}
		}()
		g, e := got()
		if e != nil {
			fails = append(fails, fmt.Sprintf("%s: error: %v", name, e))
			return
		}
		if !Equal(g, want) {
			fails = append(fails, fmt.Sprintf("%s: got %s, want %s", name, show(g), show(want)))
		}
	}

	f, err := loadVectors(dir, "canonical.json")
	if err != nil {
		return 0, nil, err
	}
	for _, c := range cases(f, "cases") {
		c := c
		check("canonical/"+fieldStr(c, "name"), func() (Value, error) {
			t, err := Canonical(vfield(c, "input"))
			if err != nil {
				return nil, err
			}
			return []Value{t, SHA256Hex(t)}, nil
		}, []Value{vfield(c, "canonical"), vfield(c, "sha256")})
	}

	if f, err = loadVectors(dir, "figures.json"); err != nil {
		return 0, nil, err
	}
	for i, c := range cases(f, "cases") {
		c := c
		check(fmt.Sprintf("figures/%d", i), func() (Value, error) {
			return strValues(Figures(fieldStr(c, "text"))), nil
		}, vfield(c, "figures"))
	}

	if f, err = loadVectors(dir, "gates.json"); err != nil {
		return 0, nil, err
	}
	g, _ := f.GetOr("cases", nil).(*Object)
	for _, c := range cases(g, "flag_unsourced") {
		c := c
		check("flag_unsourced/"+fieldStr(c, "name"), func() (Value, error) {
			view, _ := vfield(c, "view").(*Object)
			return FlagUnsourced(view, vfield(c, "allowed")), nil
		}, vfield(c, "flags"))
	}
	fns := []struct {
		name string
		fn   func(string) Value
	}{
		{"is_generic", nil},
		{"is_fallback", func(t string) Value { return IsFallback(t) }},
		{"has_placeholder_code", func(t string) Value { return HasPlaceholderCode(t) }},
		{"compliance_block", func(t string) Value { return strValues(ComplianceBlock(t, nil)) }},
		{"squash", func(t string) Value { return Squash(t) }},
	}
	for _, fn := range fns {
		fn := fn
		for i, c := range cases(g, fn.name) {
			c := c
			check(fmt.Sprintf("%s/%d", fn.name, i), func() (Value, error) {
				if fn.fn == nil { // is_generic takes any value
					return IsGeneric(vfield(c, "text")), nil
				}
				return fn.fn(fieldStr(c, "text")), nil
			}, vfield(c, "result"))
		}
	}
	for _, c := range cases(g, "synthesis_ok") {
		c := c
		check("synthesis_ok/"+fieldStr(c, "name"), func() (Value, error) {
			m, _ := vfield(c, "merge").(*Object)
			return SynthesisOK(m), nil
		}, vfield(c, "result"))
	}

	if f, err = loadVectors(dir, "schema.json"); err != nil {
		return 0, nil, err
	}
	s, _ := f.GetOr("cases", nil).(*Object)
	for _, c := range cases(s, "view") {
		c := c
		check("schema/view/"+fieldStr(c, "name"), func() (Value, error) {
			return strValues(CheckView(vfield(c, "input"))), nil
		}, vfield(c, "errors"))
	}
	for _, c := range cases(s, "merge") {
		c := c
		check("schema/merge/"+fieldStr(c, "name"), func() (Value, error) {
			return strValues(CheckMerge(vfield(c, "input"))), nil
		}, vfield(c, "errors"))
	}

	if f, err = loadVectors(dir, "runs.json"); err != nil {
		return 0, nil, err
	}
	for _, c := range cases(f, "cases") {
		name := fieldStr(c, "name")
		expect, _ := vfield(c, "expect").(*Object)
		opts := DefaultOptions()
		opts.Roster = valueStrs(vfield(c, "roster"))
		opts.Sources = valueStrs(vfield(c, "sources"))
		if ph, ok := vfield(c, "phases").([]Value); ok && len(ph) > 0 {
			phases, perr := PhasesFromValue(Obj("phases", ph))
			if perr != nil {
				fails = append(fails, fmt.Sprintf("runs/%s: phases: %v", name, perr))
			}
			opts.Phases = phases
		}
		if n, ok := vfield(c, "max_loops").(int64); ok {
			opts.MaxLoops = int(n)
		}
		if n, ok := vfield(c, "retries").(int64); ok {
			opts.Retries = int(n)
		}
		script, _ := vfield(c, "script").(*Object)
		council, cerr := NewCouncil(fieldStr(c, "idea"), NewScriptedAdapter(script), opts)
		var res *CouncilResult
		if cerr == nil {
			res = council.Run()
		}
		key := fieldStr(c, "project_key")
		for _, fieldName := range expect.Keys() {
			fieldName := fieldName
			check("runs/"+name+"/"+fieldName, func() (Value, error) {
				if cerr != nil {
					return nil, cerr
				}
				switch fieldName {
				case "council":
					return res.Value(), nil
				case "digest":
					return RunDigest(res), nil
				case "health":
					return HealthOf(res).Value(), nil
				case "stories":
					return ToStories(res, key), nil
				case "spec_md":
					return ToSpecMd(res), nil
				case "mermaid":
					return ToMermaid(res), nil
				case "trainset":
					return ToTrainset(res, false), nil
				case "trainset_all":
					return ToTrainset(res, true), nil
				case "audit":
					return AuditResult(res).Value(), nil
				}
				return nil, fmt.Errorf("unknown field %s", fieldName)
			}, expect.vals[fieldName])
		}
	}

	if f, err = loadVectors(dir, "audit.json"); err != nil {
		return 0, nil, err
	}
	a, _ := f.GetOr("cases", nil).(*Object)
	for _, c := range cases(a, "import") {
		c := c
		check("import/"+fieldStr(c, "name"), func() (Value, error) {
			res, err := FromSmartSignal(vfield(c, "input"))
			if err != nil {
				return nil, err
			}
			return res.Value(), nil
		}, vfield(c, "council"))
	}
	for _, c := range cases(a, "council") {
		c := c
		check("audit/council/"+fieldStr(c, "name"), func() (Value, error) {
			res, err := FromSmartSignal(vfield(c, "input"))
			if err != nil {
				return nil, err
			}
			return AuditResult(res).Value(), nil
		}, vfield(c, "report"))
	}
	for _, c := range cases(a, "stories") {
		c := c
		check("audit/stories/"+fieldStr(c, "name"), func() (Value, error) {
			return AuditStories(vfield(c, "input")).Value(), nil
		}, vfield(c, "report"))
	}
	for _, c := range cases(a, "trainset") {
		c := c
		check("audit/trainset/"+fieldStr(c, "name"), func() (Value, error) {
			rows, _ := vfield(c, "input").([]Value)
			return AuditTrainset(rows).Value(), nil
		}, vfield(c, "report"))
	}
	return total, fails, nil
}
