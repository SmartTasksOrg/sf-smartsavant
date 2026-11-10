package smartsavant

// JSON values as the Python reference sees them: objects keep their key order (Python dicts do), integers
// and floats stay distinct (1 is not 1.0, true is not 1), and unpaired UTF-16 surrogates survive parsing.
//
// A Value is one of: nil, bool, int64, BigInt (an integer outside the int64 range, as its decimal text),
// float64, string, []Value, *Object.
//
// Go strings hold UTF-8, which cannot encode U+D800-U+DFFF. The parser therefore maps an unpaired
// surrogate escape ("\ud800") to the code point U+10F800 + (s - 0xD800), in the last 2048 code points of
// plane 16 (private use). Every check that looks for unpaired surrogates looks for that range, and the
// writers turn it back into a "\udXXX" escape. A real character from U+10F800-U+10FFFF in the input is
// therefore treated as an unpaired surrogate (documented in README.md).

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"math"
	"sort"
	"strconv"
	"strings"
	"unicode/utf8"
)

// Value is a JSON value: nil, bool, int64, BigInt, float64, string, []Value or *Object.
type Value = any

// BigInt is an integer that does not fit in int64, kept as its decimal text.
type BigInt string

// Object is a JSON object that keeps the insertion order of its keys (as a Python dict does).
type Object struct {
	keys []string
	vals map[string]Value
}

// NewObject returns an empty object.
func NewObject() *Object { return &Object{vals: map[string]Value{}} }

// Obj builds an object from key, value pairs: Obj("a", 1, "b", "x").
func Obj(kv ...any) *Object {
	o := NewObject()
	for i := 0; i+1 < len(kv); i += 2 {
		o.Set(kv[i].(string), kv[i+1])
	}
	return o
}

// Set sets k to v; a new key goes last, an existing key keeps its position (as in Python).
func (o *Object) Set(k string, v Value) *Object {
	if _, ok := o.vals[k]; !ok {
		o.keys = append(o.keys, k)
	}
	o.vals[k] = v
	return o
}

// Get returns the value of k and whether it is present.
func (o *Object) Get(k string) (Value, bool) {
	if o == nil {
		return nil, false
	}
	v, ok := o.vals[k]
	return v, ok
}

// GetOr is dict.get(k, d): a present key with a null value gives nil, not d.
func (o *Object) GetOr(k string, d Value) Value {
	if v, ok := o.Get(k); ok {
		return v
	}
	return d
}

// Has reports whether k is present.
func (o *Object) Has(k string) bool { _, ok := o.Get(k); return ok }

// Keys returns the keys in insertion order (a copy).
func (o *Object) Keys() []string {
	if o == nil {
		return nil
	}
	return append([]string(nil), o.keys...)
}

// Len is the number of keys.
func (o *Object) Len() int {
	if o == nil {
		return 0
	}
	return len(o.keys)
}

// Copy is a shallow copy.
func (o *Object) Copy() *Object {
	out := NewObject()
	for _, k := range o.keys {
		out.Set(k, o.vals[k])
	}
	return out
}

// DeepCopy copies a Value made of the types listed for Value.
func DeepCopy(v Value) Value {
	switch x := v.(type) {
	case []Value:
		out := make([]Value, len(x))
		for i, e := range x {
			out[i] = DeepCopy(e)
		}
		return out
	case *Object:
		if x == nil {
			return nil
		}
		out := NewObject()
		for _, k := range x.keys {
			out.Set(k, DeepCopy(x.vals[k]))
		}
		return out
	}
	return v
}

// ---------------------------------------------------------------- unpaired surrogates

const surrogateBase = 0x10F800

// IsLoneSurrogate reports whether r stands for an unpaired surrogate (see the package comment).
func IsLoneSurrogate(r rune) bool { return r >= surrogateBase && r <= 0x10FFFF }

// LoneSurrogate returns the rune that stands for the surrogate code unit u (0xD800-0xDFFF).
func LoneSurrogate(u int) rune { return rune(surrogateBase + (u - 0xD800)) }

// HasLoneSurrogate reports whether s holds an unpaired surrogate.
func HasLoneSurrogate(s string) bool {
	for _, r := range s {
		if IsLoneSurrogate(r) {
			return true
		}
	}
	return false
}

// ReplaceLoneSurrogates replaces each unpaired surrogate with by.
func ReplaceLoneSurrogates(s, by string) string {
	if !HasLoneSurrogate(s) {
		return s
	}
	var b strings.Builder
	for _, r := range s {
		if IsLoneSurrogate(r) {
			b.WriteString(by)
		} else {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// ---------------------------------------------------------------- parser

// ParseJSON reads one JSON document as Python's json.loads does: objects keep their key order (a repeated
// key keeps its first position and its last value), integers stay int64 (BigInt beyond), numbers with a
// fraction or exponent are float64, NaN, Infinity and -Infinity are accepted, control characters inside
// strings are refused and unpaired surrogate escapes are kept (see the package comment).
func ParseJSON(data []byte) (Value, error) {
	if !utf8.Valid(data) {
		return nil, errors.New("JSON text is not valid UTF-8")
	}
	p := &parser{s: data}
	p.ws()
	v, err := p.value(0)
	if err != nil {
		return nil, err
	}
	p.ws()
	if p.i != len(p.s) {
		return nil, p.fail("extra data")
	}
	return v, nil
}

type parser struct {
	s []byte
	i int
}

const maxParseDepth = 1000

func (p *parser) fail(msg string) error { return fmt.Errorf("%s at byte %d", msg, p.i) }

func (p *parser) ws() {
	for p.i < len(p.s) {
		switch p.s[p.i] {
		case ' ', '\t', '\n', '\r':
			p.i++
		default:
			return
		}
	}
}

func (p *parser) lit(word string) bool {
	if strings.HasPrefix(string(p.s[p.i:min(len(p.s), p.i+len(word))]), word) {
		p.i += len(word)
		return true
	}
	return false
}

func (p *parser) value(depth int) (Value, error) {
	if depth > maxParseDepth {
		return nil, p.fail("JSON nested too deeply")
	}
	if p.i >= len(p.s) {
		return nil, p.fail("unexpected end of JSON")
	}
	switch c := p.s[p.i]; {
	case c == '{':
		p.i++
		o := NewObject()
		p.ws()
		if p.i < len(p.s) && p.s[p.i] == '}' {
			p.i++
			return o, nil
		}
		for {
			p.ws()
			if p.i >= len(p.s) || p.s[p.i] != '"' {
				return nil, p.fail("expected a string key")
			}
			k, err := p.str()
			if err != nil {
				return nil, err
			}
			p.ws()
			if p.i >= len(p.s) || p.s[p.i] != ':' {
				return nil, p.fail("expected ':'")
			}
			p.i++
			p.ws()
			v, err := p.value(depth + 1)
			if err != nil {
				return nil, err
			}
			o.Set(k, v)
			p.ws()
			if p.i < len(p.s) && p.s[p.i] == ',' {
				p.i++
				continue
			}
			if p.i < len(p.s) && p.s[p.i] == '}' {
				p.i++
				return o, nil
			}
			return nil, p.fail("expected ',' or '}'")
		}
	case c == '[':
		p.i++
		out := []Value{}
		p.ws()
		if p.i < len(p.s) && p.s[p.i] == ']' {
			p.i++
			return out, nil
		}
		for {
			p.ws()
			v, err := p.value(depth + 1)
			if err != nil {
				return nil, err
			}
			out = append(out, v)
			p.ws()
			if p.i < len(p.s) && p.s[p.i] == ',' {
				p.i++
				continue
			}
			if p.i < len(p.s) && p.s[p.i] == ']' {
				p.i++
				return out, nil
			}
			return nil, p.fail("expected ',' or ']'")
		}
	case c == '"':
		return p.str()
	case p.lit("null"):
		return nil, nil
	case p.lit("true"):
		return true, nil
	case p.lit("false"):
		return false, nil
	case p.lit("NaN"):
		return math.NaN(), nil
	case p.lit("Infinity"):
		return math.Inf(1), nil
	case p.lit("-Infinity"):
		return math.Inf(-1), nil
	case c == '-' || (c >= '0' && c <= '9'):
		return p.number()
	}
	return nil, p.fail("unexpected character")
}

func (p *parser) number() (Value, error) {
	start := p.i
	if p.s[p.i] == '-' {
		p.i++
	}
	digits := func() int {
		n := 0
		for p.i < len(p.s) && p.s[p.i] >= '0' && p.s[p.i] <= '9' {
			p.i++
			n++
		}
		return n
	}
	if p.i < len(p.s) && p.s[p.i] == '0' {
		p.i++
	} else if digits() == 0 {
		return nil, p.fail("bad number")
	}
	isFloat := false
	if p.i < len(p.s) && p.s[p.i] == '.' {
		save := p.i
		p.i++
		if digits() == 0 {
			p.i = save // "1." is "1" followed by extra data, as in Python
		} else {
			isFloat = true
		}
	}
	if p.i < len(p.s) && (p.s[p.i] == 'e' || p.s[p.i] == 'E') {
		save := p.i
		p.i++
		if p.i < len(p.s) && (p.s[p.i] == '+' || p.s[p.i] == '-') {
			p.i++
		}
		if digits() == 0 {
			p.i = save
		} else {
			isFloat = true
		}
	}
	text := string(p.s[start:p.i])
	if isFloat {
		f, err := strconv.ParseFloat(text, 64)
		if err != nil && !errors.Is(err, strconv.ErrRange) {
			return nil, p.fail("bad number")
		}
		return f, nil
	}
	if n, err := strconv.ParseInt(text, 10, 64); err == nil {
		return n, nil
	}
	t := strings.TrimLeft(strings.TrimPrefix(text, "-"), "0")
	if strings.HasPrefix(text, "-") {
		t = "-" + t
	}
	return BigInt(t), nil
}

func hex4(b []byte) (int, bool) {
	if len(b) < 4 {
		return 0, false
	}
	n, err := strconv.ParseUint(string(b[:4]), 16, 32)
	if err != nil {
		return 0, false
	}
	return int(n), true
}

func (p *parser) str() (string, error) {
	p.i++ // opening quote
	var b strings.Builder
	for {
		if p.i >= len(p.s) {
			return "", p.fail("unterminated string")
		}
		c := p.s[p.i]
		switch {
		case c == '"':
			p.i++
			return b.String(), nil
		case c < 0x20:
			return "", p.fail("invalid control character in string")
		case c != '\\':
			r, size := utf8.DecodeRune(p.s[p.i:])
			b.WriteRune(r)
			p.i += size
			continue
		}
		p.i++
		if p.i >= len(p.s) {
			return "", p.fail("unterminated string")
		}
		e := p.s[p.i]
		p.i++
		switch e {
		case '"', '\\', '/':
			b.WriteByte(e)
		case 'b':
			b.WriteByte('\b')
		case 'f':
			b.WriteByte('\f')
		case 'n':
			b.WriteByte('\n')
		case 'r':
			b.WriteByte('\r')
		case 't':
			b.WriteByte('\t')
		case 'u':
			u, ok := hex4(p.s[p.i:])
			if !ok {
				return "", p.fail("bad \\u escape")
			}
			p.i += 4
			switch {
			case u >= 0xD800 && u <= 0xDBFF:
				if p.i+1 < len(p.s) && p.s[p.i] == '\\' && p.s[p.i+1] == 'u' {
					if lo, ok := hex4(p.s[p.i+2:]); ok && lo >= 0xDC00 && lo <= 0xDFFF {
						p.i += 6
						b.WriteRune(rune(0x10000 + (u-0xD800)<<10 + (lo - 0xDC00)))
						continue
					}
				}
				b.WriteRune(LoneSurrogate(u))
			case u >= 0xDC00 && u <= 0xDFFF:
				b.WriteRune(LoneSurrogate(u))
			default:
				b.WriteRune(rune(u))
			}
		default:
			return "", p.fail("invalid escape")
		}
	}
}

// ---------------------------------------------------------------- canonical JSON

func canonicalStr(b *strings.Builder, s string) {
	b.WriteByte('"')
	for _, r := range s {
		switch {
		case r == '"':
			b.WriteString(`\"`)
		case r == '\\':
			b.WriteString(`\\`)
		case r == '\b':
			b.WriteString(`\b`)
		case r == '\f':
			b.WriteString(`\f`)
		case r == '\n':
			b.WriteString(`\n`)
		case r == '\r':
			b.WriteString(`\r`)
		case r == '\t':
			b.WriteString(`\t`)
		case r < 0x20:
			fmt.Fprintf(b, `\u%04x`, r)
		case IsLoneSurrogate(r):
			fmt.Fprintf(b, `\u%04x`, int(r)-surrogateBase+0xD800)
		default:
			b.WriteRune(r)
		}
	}
	b.WriteByte('"')
}

// Canonical returns the canonical JSON text of v (spec/SEMANTICS.md section 6): keys sorted by bytes and
// ASCII only, no whitespace, integers only, and only '"', '\' and characters below U+0020 escaped.
// Floats, non-ASCII keys and other types are an error. (An unpaired surrogate, which Python cannot
// encode as UTF-8 at all, is written as a \udXXX escape.)
func Canonical(v Value) (string, error) {
	var b strings.Builder
	if err := canonical(&b, v); err != nil {
		return "", err
	}
	return b.String(), nil
}

func canonical(b *strings.Builder, v Value) error {
	switch x := v.(type) {
	case nil:
		b.WriteString("null")
	case bool:
		if x {
			b.WriteString("true")
		} else {
			b.WriteString("false")
		}
	case int64:
		b.WriteString(strconv.FormatInt(x, 10))
	case int:
		b.WriteString(strconv.Itoa(x))
	case BigInt:
		b.WriteString(string(x))
	case string:
		canonicalStr(b, x)
	case []Value:
		b.WriteByte('[')
		for i, e := range x {
			if i > 0 {
				b.WriteByte(',')
			}
			if err := canonical(b, e); err != nil {
				return err
			}
		}
		b.WriteByte(']')
	case *Object:
		keys := x.Keys()
		for _, k := range keys {
			for i := 0; i < len(k); i++ {
				if k[i] >= 0x80 {
					return errors.New("canonical JSON object keys must be ASCII strings")
				}
			}
		}
		sort.Strings(keys)
		b.WriteByte('{')
		for i, k := range keys {
			if i > 0 {
				b.WriteByte(',')
			}
			canonicalStr(b, k)
			b.WriteByte(':')
			if err := canonical(b, x.vals[k]); err != nil {
				return err
			}
		}
		b.WriteByte('}')
	default:
		return fmt.Errorf("canonical JSON does not encode %s (floats are not allowed)", pyTypeName(v))
	}
	return nil
}

// SHA256Hex is the lowercase hex SHA-256 of the UTF-8 bytes of s.
func SHA256Hex(s string) string {
	h := sha256.Sum256([]byte(s))
	return hex.EncodeToString(h[:])
}

// ---------------------------------------------------------------- Python's json.dumps

// PyDumps is Python's json.dumps(v, ensure_ascii=False[, sort_keys=True]): ", " and ": " separators,
// only '"', '\', \n, \r, \t, \b, \f and the other characters below U+0020 escaped, floats as repr(), NaN
// and Infinity as Python writes them. Keys are sorted by code point when sortKeys is set.
func PyDumps(v Value, sortKeys bool) string {
	var b strings.Builder
	pyDumps(&b, v, sortKeys)
	return b.String()
}

func pyJSONStr(b *strings.Builder, s string) {
	b.WriteByte('"')
	for _, r := range s {
		switch {
		case r == '"':
			b.WriteString(`\"`)
		case r == '\\':
			b.WriteString(`\\`)
		case r == '\n':
			b.WriteString(`\n`)
		case r == '\r':
			b.WriteString(`\r`)
		case r == '\t':
			b.WriteString(`\t`)
		case r == '\b':
			b.WriteString(`\b`)
		case r == '\f':
			b.WriteString(`\f`)
		case r < 0x20:
			fmt.Fprintf(b, `\u%04x`, r)
		case IsLoneSurrogate(r):
			fmt.Fprintf(b, `\u%04x`, int(r)-surrogateBase+0xD800)
		default:
			b.WriteRune(r)
		}
	}
	b.WriteByte('"')
}

func pyDumps(b *strings.Builder, v Value, sortKeys bool) {
	switch x := v.(type) {
	case nil:
		b.WriteString("null")
	case bool:
		if x {
			b.WriteString("true")
		} else {
			b.WriteString("false")
		}
	case int64:
		b.WriteString(strconv.FormatInt(x, 10))
	case int:
		b.WriteString(strconv.Itoa(x))
	case BigInt:
		b.WriteString(string(x))
	case float64:
		switch {
		case math.IsNaN(x):
			b.WriteString("NaN")
		case math.IsInf(x, 1):
			b.WriteString("Infinity")
		case math.IsInf(x, -1):
			b.WriteString("-Infinity")
		default:
			b.WriteString(pyFloat(x))
		}
	case string:
		pyJSONStr(b, x)
	case []Value:
		b.WriteByte('[')
		for i, e := range x {
			if i > 0 {
				b.WriteString(", ")
			}
			pyDumps(b, e, sortKeys)
		}
		b.WriteByte(']')
	case *Object:
		keys := x.Keys()
		if sortKeys {
			sort.Strings(keys) // UTF-8 byte order is code point order
		}
		b.WriteByte('{')
		for i, k := range keys {
			if i > 0 {
				b.WriteString(", ")
			}
			pyJSONStr(b, k)
			b.WriteString(": ")
			pyDumps(b, x.vals[k], sortKeys)
		}
		b.WriteByte('}')
	default:
		b.WriteString(fmt.Sprintf("%q", fmt.Sprint(v)))
	}
}

// pyFloat is repr(float) for a finite float: the shortest text that reads back, in exponent form below
// 1e-4 and from 1e16, always with a '.0' or an exponent.
func pyFloat(f float64) string {
	if f == 0 {
		if math.Signbit(f) {
			return "-0.0"
		}
		return "0.0"
	}
	e := strconv.FormatFloat(f, 'e', -1, 64) // d.ddde±XX
	mant, exps, _ := strings.Cut(e, "e")
	exp, _ := strconv.Atoi(exps)
	if exp >= -4 && exp < 16 {
		s := strconv.FormatFloat(f, 'f', -1, 64)
		if !strings.Contains(s, ".") {
			s += ".0"
		}
		return s
	}
	sign := "+"
	if exp < 0 {
		sign = "-"
		exp = -exp
	}
	return fmt.Sprintf("%se%s%02d", mant, sign, exp)
}

// ---------------------------------------------------------------- equality

// Equal compares two JSON values as the conformance vectors require: key order does not matter, an
// integer equals the same number as a float, and true is not 1.
func Equal(a, b Value) bool {
	switch x := a.(type) {
	case nil:
		return b == nil
	case bool:
		y, ok := b.(bool)
		return ok && x == y
	case string:
		y, ok := b.(string)
		return ok && x == y
	case int64, int, float64, BigInt:
		return numEqual(a, b)
	case []Value:
		y, ok := b.([]Value)
		if !ok || len(x) != len(y) {
			return false
		}
		for i := range x {
			if !Equal(x[i], y[i]) {
				return false
			}
		}
		return true
	case *Object:
		y, ok := b.(*Object)
		if !ok || x.Len() != y.Len() {
			return false
		}
		for _, k := range x.keys {
			w, ok := y.Get(k)
			if !ok || !Equal(x.vals[k], w) {
				return false
			}
		}
		return true
	}
	return false
}

func numEqual(a, b Value) bool {
	f := func(v Value) (float64, string, bool) {
		switch n := v.(type) {
		case int64:
			return float64(n), strconv.FormatInt(n, 10), true
		case int:
			return float64(n), strconv.Itoa(n), true
		case BigInt:
			return math.NaN(), string(n), true
		case float64:
			return n, "", true
		}
		return 0, "", false
	}
	fa, ia, oka := f(a)
	fb, ib, okb := f(b)
	if !oka || !okb {
		return false
	}
	if ia != "" && ib != "" {
		return ia == ib
	}
	return fa == fb
}
