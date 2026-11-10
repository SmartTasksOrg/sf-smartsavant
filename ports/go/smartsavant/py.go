package smartsavant

// The Python behaviour the reference relies on: code-point lengths and slices, str(), repr(), truth
// values, str.lower() and iteration over a value.

import (
	"fmt"
	"math"
	"sort"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"
)

// Len is len(s) in code points.
func Len(s string) int { return utf8.RuneCountInString(s) }

// Head is s[:n] in code points.
func Head(s string, n int) string {
	if len(s) <= n {
		return s
	}
	i := 0
	for pos := range s {
		if i == n {
			return s[:pos]
		}
		i++
	}
	return s
}

// pyLower is str.lower(): Unicode lower case, with U+0130 as "i̇" as Python writes it.
func pyLower(s string) string {
	if !strings.ContainsRune(s, 0x130) {
		return strings.ToLower(s)
	}
	return strings.ToLower(strings.ReplaceAll(s, "İ", "i̇"))
}

// pyStrip is str.strip(chars).
func pyStrip(s, chars string) string {
	return strings.TrimFunc(s, func(r rune) bool { return strings.ContainsRune(chars, r) })
}

func pyTypeName(v Value) string {
	switch v.(type) {
	case nil:
		return "NoneType"
	case bool:
		return "bool"
	case int64, int, BigInt:
		return "int"
	case float64:
		return "float"
	case string:
		return "str"
	case []Value:
		return "list"
	case *Object:
		return "dict"
	}
	return fmt.Sprintf("%T", v)
}

// Truthy is bool(v).
func Truthy(v Value) bool {
	switch x := v.(type) {
	case nil:
		return false
	case bool:
		return x
	case int64:
		return x != 0
	case int:
		return x != 0
	case BigInt:
		return true
	case float64:
		return x != 0
	case string:
		return x != ""
	case []Value:
		return len(x) > 0
	case *Object:
		return x.Len() > 0
	}
	return true
}

// pyIter is iteration as Python does it: a list gives its items, a string its characters, a dict its
// keys; anything else (where Python would raise) gives nothing.
func pyIter(v Value) []Value {
	switch x := v.(type) {
	case []Value:
		return x
	case string:
		out := make([]Value, 0, len(x))
		for _, r := range x {
			out = append(out, string(r))
		}
		return out
	case *Object:
		out := make([]Value, 0, x.Len())
		for _, k := range x.keys {
			out = append(out, k)
		}
		return out
	}
	return nil
}

// Str is str(v): a string as is, anything else as repr().
func Str(v Value) string {
	if s, ok := v.(string); ok {
		return s
	}
	return Repr(v)
}

// Repr is Python's repr() of a JSON value.
func Repr(v Value) string {
	switch x := v.(type) {
	case nil:
		return "None"
	case bool:
		if x {
			return "True"
		}
		return "False"
	case int64:
		return strconv.FormatInt(x, 10)
	case int:
		return strconv.Itoa(x)
	case BigInt:
		return string(x)
	case float64:
		switch {
		case math.IsNaN(x):
			return "nan"
		case math.IsInf(x, 1):
			return "inf"
		case math.IsInf(x, -1):
			return "-inf"
		}
		return pyFloat(x)
	case string:
		return reprStr(x)
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			parts[i] = Repr(e)
		}
		return "[" + strings.Join(parts, ", ") + "]"
	case *Object:
		parts := make([]string, 0, x.Len())
		for _, k := range x.keys {
			parts = append(parts, reprStr(k)+": "+Repr(x.vals[k]))
		}
		return "{" + strings.Join(parts, ", ") + "}"
	}
	return fmt.Sprint(v)
}

// reprStr is repr(str): the quote Python picks, and non-printable characters escaped. Printability
// comes from Go's unicode.IsPrint, whose Unicode version may differ from Python's.
func reprStr(s string) string {
	q := '\''
	if strings.ContainsRune(s, '\'') && !strings.ContainsRune(s, '"') {
		q = '"'
	}
	var b strings.Builder
	b.WriteRune(q)
	for _, r := range s {
		switch {
		case r == q || r == '\\':
			b.WriteByte('\\')
			b.WriteRune(r)
		case r == '\n':
			b.WriteString(`\n`)
		case r == '\r':
			b.WriteString(`\r`)
		case r == '\t':
			b.WriteString(`\t`)
		case IsLoneSurrogate(r):
			fmt.Fprintf(&b, `\u%04x`, int(r)-surrogateBase+0xD800)
		case r == ' ' || unicode.IsPrint(r):
			b.WriteRune(r)
		case r < 0x100:
			fmt.Fprintf(&b, `\x%02x`, r)
		case r < 0x10000:
			fmt.Fprintf(&b, `\u%04x`, r)
		default:
			fmt.Fprintf(&b, `\U%08x`, r)
		}
	}
	b.WriteRune(q)
	return b.String()
}

// sortedStrings sorts by code point (UTF-8 byte order is code point order).
func sortedStrings(xs []string) []string {
	out := append([]string(nil), xs...)
	sort.Strings(out)
	return out
}

func strsToValues(xs []string) []Value {
	out := make([]Value, len(xs))
	for i, x := range xs {
		out[i] = x
	}
	return out
}
