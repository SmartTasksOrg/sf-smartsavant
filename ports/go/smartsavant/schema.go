package smartsavant

// Validation of what an adapter returns, and of council files (spec/SEMANTICS.md section 3; vectors
// spec/conformance/schema.json). The error strings and their order are part of the contract.

import (
	"fmt"
	"math"
	"reflect"
	"sort"
	"strings"
)

// Limits of the adapter contract.
const (
	MaxStr   = 8000
	MaxDelta = 20000
	MaxItems = 20
	MaxItem  = 2000
	MaxDepth = 32
	MaxTotal = 1000000 // code points of all strings in one answer, keys included
)

var w5h1Keys = []string{"who", "what", "where", "when", "why", "how"}
var viewKeys = []string{"w5h1", "done_so_far", "missing", "devils_advocate", "contribution", "sources"}
var mergeKeys = []string{"merge_summary", "decision", "resolved", "open_issues", "next_steps", "reversals",
	"spec_delta", "loop_recommended"}
var mergeRequired = []string{"merge_summary", "decision", "resolved", "open_issues", "next_steps", "spec_delta",
	"loop_recommended"}

// NotPlainData is the error of Plain.
type NotPlainData struct{ Msg string }

func (e *NotPlainData) Error() string { return e.Msg }

// ErrorName is the exception name used in recorded errors.
func (e *NotPlainData) ErrorName() string { return "NotPlainData" }

// Plain copies an adapter answer into a JSON Value (schema.plain in Python), before anything else, so
// that what is validated is exactly what is used. Accepted: nil, bool, Go integers, float32/float64,
// string, BigInt, *Object, slices of any of these, and maps with string keys (copied in sorted key order).
// Refused with *NotPlainData: other types (structs, pointers, channels, functions, byte slices ...),
// nesting deeper than 32 and more than 1,000,000 code points of text (keys included).
func Plain(obj any) (Value, error) {
	budget := MaxTotal
	tooMuch := &NotPlainData{"more than 1,000,000 characters of text"}
	var walk func(x any, depth int) (Value, error)
	walk = func(x any, depth int) (Value, error) {
		if depth > MaxDepth {
			return nil, &NotPlainData{"nested deeper than 32 levels"}
		}
		switch v := x.(type) {
		case nil:
			return nil, nil
		case bool:
			return v, nil
		case int64:
			return v, nil
		case int:
			return int64(v), nil
		case int8:
			return int64(v), nil
		case int16:
			return int64(v), nil
		case int32:
			return int64(v), nil
		case uint8:
			return int64(v), nil
		case uint16:
			return int64(v), nil
		case uint32:
			return int64(v), nil
		case uint:
			if uint64(v) > math.MaxInt64 {
				return BigInt(fmt.Sprint(v)), nil
			}
			return int64(v), nil
		case uint64:
			if v > math.MaxInt64 {
				return BigInt(fmt.Sprint(v)), nil
			}
			return int64(v), nil
		case BigInt:
			return v, nil
		case float64:
			return v, nil
		case float32:
			return float64(v), nil
		case string:
			budget -= Len(v)
			if budget < 0 {
				return nil, tooMuch
			}
			return v, nil
		case *Object:
			if v == nil {
				return nil, nil
			}
			out := NewObject()
			for _, k := range v.keys {
				budget -= Len(k)
				e, err := walk(v.vals[k], depth+1)
				if err != nil {
					return nil, err
				}
				out.Set(k, e)
			}
			if budget < 0 {
				return nil, tooMuch
			}
			return out, nil
		case []byte:
			return nil, &NotPlainData{"bytes is not JSON data"}
		}
		rv := reflect.ValueOf(x)
		switch rv.Kind() {
		case reflect.Slice:
			out := make([]Value, 0, rv.Len())
			for i := 0; i < rv.Len(); i++ {
				e, err := walk(rv.Index(i).Interface(), depth+1)
				if err != nil {
					return nil, err
				}
				out = append(out, e)
			}
			return out, nil
		case reflect.Map:
			if rv.Type().Key().Kind() != reflect.String {
				return nil, &NotPlainData{"object keys must be strings"}
			}
			keys := make([]string, 0, rv.Len())
			for _, k := range rv.MapKeys() {
				keys = append(keys, k.String())
			}
			sort.Strings(keys)
			out := NewObject()
			for _, k := range keys {
				budget -= Len(k)
				e, err := walk(rv.MapIndex(reflect.ValueOf(k).Convert(rv.Type().Key())).Interface(), depth+1)
				if err != nil {
					return nil, err
				}
				out.Set(k, e)
			}
			if budget < 0 {
				return nil, tooMuch
			}
			return out, nil
		}
		return nil, &NotPlainData{fmt.Sprintf("%T is not JSON data", x)}
	}
	return walk(obj, 0)
}

// showKey is an unknown key as shown in an error: control characters dropped, unpaired surrogates as
// U+FFFD, then at most 40 code points.
func showKey(k string) string {
	var b strings.Builder
	for _, r := range k {
		switch {
		case r < 32 || (r >= 127 && r < 160):
		case IsLoneSurrogate(r): // error text is recorded: it must stay encodable
			b.WriteRune('\uFFFD')
		default:
			b.WriteRune(r)
		}
	}
	s := b.String()
	if Len(s) > 40 {
		return Head(s, 40) + "..."
	}
	return s
}

func checkStr(errs *[]string, where string, v Value, limit int, nonempty bool) {
	s, ok := v.(string)
	switch {
	case !ok:
		*errs = append(*errs, where+" must be a string")
	case Len(s) > limit:
		*errs = append(*errs, fmt.Sprintf("%s is longer than %d characters", where, limit))
	case strings.ContainsRune(s, 0):
		*errs = append(*errs, where+" contains a NUL character")
	case HasLoneSurrogate(s):
		*errs = append(*errs, where+" contains an unpaired surrogate")
	case nonempty && blankStr(s):
		*errs = append(*errs, where+" must not be empty")
	}
}

func isSourceID(s string) bool {
	if len(s) < 1 || len(s) > 200 {
		return false
	}
	for i := 0; i < len(s); i++ {
		c := s[i]
		if !(isWord(c) && c != '_' || strings.IndexByte("._:/#@-", c) >= 0) {
			return false
		}
	}
	return true
}

func checkList(errs *[]string, where string, v Value, sourceIDs bool) {
	l, ok := v.([]Value)
	if !ok {
		*errs = append(*errs, where+" must be a list of strings")
		return
	}
	if len(l) > MaxItems {
		*errs = append(*errs, fmt.Sprintf("%s has more than %d items", where, MaxItems))
	}
	for i, x := range l {
		if i > MaxItems {
			break
		}
		s, isStr := x.(string)
		switch {
		case !isStr || blankStr(s):
			*errs = append(*errs, fmt.Sprintf("%s[%d] must be a non-empty string", where, i))
		case Len(s) > MaxItem:
			*errs = append(*errs, fmt.Sprintf("%s[%d] is longer than %d characters", where, i, MaxItem))
		case sourceIDs && !isSourceID(s):
			*errs = append(*errs, fmt.Sprintf("%s[%d] is not a valid source id", where, i))
		case strings.ContainsRune(s, 0):
			*errs = append(*errs, fmt.Sprintf("%s[%d] contains a NUL character", where, i))
		case HasLoneSurrogate(s):
			*errs = append(*errs, fmt.Sprintf("%s[%d] contains an unpaired surrogate", where, i))
		}
	}
}

func contains(xs []string, s string) bool {
	for _, x := range xs {
		if x == s {
			return true
		}
	}
	return false
}

func keyErrors(o *Object, allowed, required []string) (unknown string, missing string) {
	var extra, miss []string
	for _, k := range o.keys {
		if !contains(allowed, k) {
			extra = append(extra, k)
		}
	}
	for _, k := range required {
		if !o.Has(k) {
			miss = append(miss, k)
		}
	}
	if len(extra) > 0 {
		extra = sortedStrings(extra)
		shown := make([]string, 0, 5)
		for i, k := range extra {
			if i == 5 {
				break
			}
			shown = append(shown, showKey(k))
		}
		unknown = fmt.Sprintf("unknown keys (%d): %s", len(extra), strings.Join(shown, ", "))
	}
	if len(miss) > 0 {
		missing = "missing keys: " + strings.Join(sortedStrings(miss), ", ")
	}
	return
}

// CheckView returns the errors of a view answer ([] when valid), first problem first.
func CheckView(v Value) []string {
	errs := []string{}
	o, ok := v.(*Object)
	if !ok {
		return []string{"a view must be a JSON object"}
	}
	unknown, missing := keyErrors(o, viewKeys, viewKeys)
	if unknown != "" {
		errs = append(errs, unknown)
	}
	if missing != "" {
		return append(errs, missing)
	}
	w, isObj := o.vals["w5h1"].(*Object)
	if !isObj || w.Len() != len(w5h1Keys) || !hasAll(w, w5h1Keys) {
		errs = append(errs, "w5h1 must be an object with exactly who, what, where, when, why, how")
	} else {
		for _, k := range w5h1Keys {
			checkStr(&errs, "w5h1."+k, w.vals[k], MaxStr, false)
		}
	}
	checkStr(&errs, "done_so_far", o.vals["done_so_far"], MaxStr, false)
	if _, isStr := o.vals["missing"].(string); isStr {
		checkStr(&errs, "missing", o.vals["missing"], MaxStr, false)
	} else {
		checkList(&errs, "missing", o.vals["missing"], false)
	}
	checkStr(&errs, "devils_advocate", o.vals["devils_advocate"], MaxStr, true)
	checkStr(&errs, "contribution", o.vals["contribution"], MaxStr, false)
	checkList(&errs, "sources", o.vals["sources"], true)
	return errs
}

func hasAll(o *Object, keys []string) bool {
	for _, k := range keys {
		if !o.Has(k) {
			return false
		}
	}
	return true
}

// CheckMerge returns the errors of a merge answer ([] when valid), first problem first.
func CheckMerge(m Value) []string {
	errs := []string{}
	o, ok := m.(*Object)
	if !ok {
		return []string{"a merge must be a JSON object"}
	}
	unknown, missing := keyErrors(o, mergeKeys, mergeRequired)
	if unknown != "" {
		errs = append(errs, unknown)
	}
	if missing != "" {
		return append(errs, missing)
	}
	checkStr(&errs, "merge_summary", o.vals["merge_summary"], MaxStr, true)
	checkStr(&errs, "decision", o.vals["decision"], MaxStr, false)
	for _, k := range []string{"resolved", "open_issues", "next_steps"} {
		checkList(&errs, k, o.vals[k], false)
	}
	if o.Has("reversals") {
		checkList(&errs, "reversals", o.vals["reversals"], false)
	}
	checkStr(&errs, "spec_delta", o.vals["spec_delta"], MaxDelta, false)
	if _, isBool := o.vals["loop_recommended"].(bool); !isBool {
		errs = append(errs, "loop_recommended must be true or false")
	}
	return errs
}

// NormalizeView returns a copy of a valid view with missing always a list.
func NormalizeView(v *Object) *Object {
	out := v.Copy()
	if s, ok := out.vals["missing"].(string); ok {
		if blankStr(s) {
			out.Set("missing", []Value{})
		} else {
			out.Set("missing", []Value{s})
		}
	}
	return out
}

// ---------------------------------------------------------------- council files

var statusV = []string{"ok", "failed", "offline"}
var statusJ = []string{"full", "thin", "recovered", "failed", "offline", "no_views"}
var topKeys = []string{"schema", "idea", "adapter", "synthetic", "stakeholders", "sources", "rounds",
	"decision_tree", "locked_decisions", "flags"}

type field struct{ name, kind string }

var viewRec = []field{{"stakeholder", "s"}, {"role", "s"}, {"status", "s"}, {"w5h1", "w"}, {"done_so_far", "s"},
	{"missing", "l"}, {"devils_advocate", "s"}, {"contribution", "s"}, {"sources", "l"}, {"error", "s"}}
var jurorRec = []field{{"synthesis_status", "s"}, {"merge_summary", "s"}, {"decision", "s"}, {"resolved", "l"},
	{"open_issues", "l"}, {"next_steps", "l"}, {"reversals", "l"}, {"spec_delta", "s"}, {"loop_recommended", "b"},
	{"error", "s"}}
var roundRec = []field{{"step_id", "s"}, {"step_title", "s"}, {"iteration", "i"}, {"question", "s"}, {"views", "L"},
	{"juror", "J"}}
var nodeRec = []field{{"step_id", "s"}, {"iteration", "i"}, {"branches", "l"}, {"decision", "s"}, {"next_steps", "l"}}

func goodStr(v Value) bool { s, ok := v.(string); return ok && !HasLoneSurrogate(s) }

func goodStrList(v Value) bool {
	l, ok := v.([]Value)
	if !ok {
		return false
	}
	for _, x := range l {
		if !goodStr(x) {
			return false
		}
	}
	return true
}

func typeOK(kind string, v Value) bool {
	switch kind {
	case "s":
		return goodStr(v)
	case "i":
		n, ok := v.(int64)
		return ok && n >= 1
	case "b":
		_, ok := v.(bool)
		return ok
	case "l":
		return goodStrList(v)
	case "w":
		o, ok := v.(*Object)
		if !ok || o.Len() != len(w5h1Keys) || !hasAll(o, w5h1Keys) {
			return false
		}
		for _, k := range o.keys {
			if !goodStr(o.vals[k]) {
				return false
			}
		}
		return true
	case "L":
		_, ok := v.([]Value)
		return ok
	case "J":
		_, ok := v.(*Object)
		return ok
	}
	return false
}

func typed(errs *[]string, where string, d Value, spec []field) {
	o, ok := d.(*Object)
	names := make([]string, len(spec))
	for i, f := range spec {
		names[i] = f.name
	}
	if !ok || o.Len() != len(spec) || !hasAll(o, names) {
		*errs = append(*errs, where+" must have exactly the keys "+strings.Join(names, ", "))
		return
	}
	for _, f := range spec {
		if !typeOK(f.kind, o.vals[f.name]) {
			*errs = append(*errs, fmt.Sprintf("%s.%s has the wrong type", where, f.name))
		}
	}
}

// CheckCouncil checks a council.json written by an engine of this package: exact keys and JSON types
// everywhere, known statuses, no unpaired surrogates. At most 50 errors.
func CheckCouncil(d Value) []string {
	errs := []string{}
	o, ok := d.(*Object)
	if !ok {
		return []string{"a council file must be a JSON object"}
	}
	if s, _ := o.GetOr("schema", nil).(string); s != "smartsavant.council/1" {
		return []string{`schema must be "smartsavant.council/1"`}
	}
	if o.Len() != len(topKeys) || !hasAll(o, topKeys) {
		return []string{"top-level keys must be exactly " + strings.Join(sortedStrings(topKeys), ", ")}
	}
	for _, k := range []string{"idea", "adapter"} {
		if !goodStr(o.vals[k]) {
			errs = append(errs, k+" must be a string")
		}
	}
	if _, isBool := o.vals["synthetic"].(bool); !isBool {
		errs = append(errs, "synthetic must be true or false")
	}
	for _, k := range []string{"stakeholders", "sources", "locked_decisions"} {
		if !goodStrList(o.vals[k]) {
			errs = append(errs, k+" must be a list of strings")
		}
	}
	rounds, ok1 := o.vals["rounds"].([]Value)
	tree, ok2 := o.vals["decision_tree"].([]Value)
	flags, ok3 := o.vals["flags"].([]Value)
	if !ok1 || !ok2 || !ok3 {
		return append(errs, "rounds, decision_tree and flags must be lists")
	}
	if len(tree) != len(rounds) {
		errs = append(errs, "decision_tree must have one node per round")
	}
	for i, r := range rounds {
		w := fmt.Sprintf("rounds[%d]", i)
		typed(&errs, w, r, roundRec)
		ro, isObj := r.(*Object)
		if !isObj {
			continue
		}
		views, okV := ro.GetOr("views", nil).([]Value)
		juror, okJ := ro.GetOr("juror", nil).(*Object)
		if !okV || !okJ {
			continue
		}
		for j, v := range views {
			typed(&errs, fmt.Sprintf("%s.views[%d]", w, j), v, viewRec)
			if vo, isObj := v.(*Object); isObj {
				if s, isStr := vo.GetOr("status", nil).(string); !isStr || !contains(statusV, s) {
					errs = append(errs, fmt.Sprintf("%s.views[%d].status must be one of %s", w, j, strings.Join(statusV, ", ")))
				}
			}
		}
		typed(&errs, w+".juror", juror, jurorRec)
		if s, isStr := juror.GetOr("synthesis_status", nil).(string); !isStr || !contains(statusJ, s) {
			errs = append(errs, fmt.Sprintf("%s.juror.synthesis_status must be one of %s", w, strings.Join(statusJ, ", ")))
		}
	}
	for i, n := range tree {
		typed(&errs, fmt.Sprintf("decision_tree[%d]", i), n, nodeRec)
	}
	for i, f := range flags {
		fo, isObj := f.(*Object)
		good := isObj
		if isObj {
			for _, k := range fo.keys {
				x := fo.vals[k]
				if _, isInt := x.(int64); !(goodStr(x) || isInt) {
					good = false
				}
			}
		}
		if !good {
			errs = append(errs, fmt.Sprintf("flags[%d] must be an object of strings and integers", i))
		}
	}
	if len(errs) > 50 {
		errs = errs[:50]
	}
	return errs
}
