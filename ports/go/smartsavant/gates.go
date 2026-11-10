package smartsavant

// Quality gates (spec/SEMANTICS.md section 5; vectors spec/conformance/gates.json and figures.json).
//
// Every pattern of the Python reference is an ASCII pattern (\s is [ \t\n\r\f\v], \w is [A-Za-z0-9_],
// case-insensitive matching folds ASCII letters only). Go's regexp has no lookbehind and folds non-ASCII
// letters (the Kelvin sign matches k), so the patterns are written out as small matchers over the UTF-8
// bytes: a byte of a multi-byte character is never an ASCII letter, digit or space, which is exactly how
// the ASCII patterns treat a non-ASCII character.

import "strings"

func isWS(c byte) bool {
	return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == '\v'
}

func isWord(c byte) bool {
	return c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '_'
}

func isDigit(c byte) bool { return c >= '0' && c <= '9' }

func lowerASCII(c byte) byte {
	if c >= 'A' && c <= 'Z' {
		return c + 32
	}
	return c
}

// asciiLower lowers ASCII letters only (lengths and positions are kept).
func asciiLower(s string) string {
	b := []byte(s)
	for i, c := range b {
		b[i] = lowerASCII(c)
	}
	return string(b)
}

// hasCI reports whether s has the lowercase ASCII literal lit at byte i, ASCII letters folded.
func hasCI(s string, i int, lit string) bool {
	if i < 0 || i+len(lit) > len(s) {
		return false
	}
	for j := 0; j < len(lit); j++ {
		if lowerASCII(s[i+j]) != lit[j] {
			return false
		}
	}
	return true
}

func skipWS(s string, i int) int {
	for i < len(s) && isWS(s[i]) {
		i++
	}
	return i
}

// Squash replaces runs of [ \t\n\r\f\v] with one space and trims.
func Squash(text string) string {
	return strings.Join(strings.FieldsFunc(text, func(r rune) bool { return r < 0x80 && isWS(byte(r)) }), " ")
}

// Blank is true for a non-string, or a string empty or made only of [ \t\n\r\f\v].
func Blank(v Value) bool {
	s, ok := v.(string)
	return !ok || blankStr(s)
}

func blankStr(s string) bool { return skipWS(s, 0) == len(s) }

// ---------------------------------------------------------------- figures

var figUnits = []string{"%", "percent", "ms", "µs", "us", "ns", "s", "sec", "secs", "seconds", "second", "min",
	"mins", "minutes", "minute", "h", "hrs", "hr", "hours", "hour", "days", "day", "weeks", "week", "months", "month",
	"years", "year", "x", "k", "K", "m", "M", "b", "B", "bn", "mn", "GB", "MB", "KB", "TB", "GiB", "MiB", "KiB", "TiB",
	"kpps", "pps", "rps", "qps", "tps", "req/s", "tokens", "token", "users", "user", "agents", "agent", "USD", "EUR",
	"GBP"}

var figCurrencies = []string{"£", "$", "€"}

func digitsFrom(s string, i int) int {
	j := i
	for j < len(s) && isDigit(s[j]) {
		j++
	}
	return j - i
}

// numEnds lists the ends of the number part starting at i, in the order Python's backtracking tries them:
// \d{1,3}(?:,\d{3})+(?:\.\d+)? | \d+\.\d+ | \d+
func numEnds(s string, i int) []int {
	var out []int
	d := digitsFrom(s, i)
	if d == 0 {
		return nil
	}
	for k := min(3, d); k >= 1; k-- { // \d{1,3}
		// (?:,\d{3})+ greedy: collect the ends after each repetition
		var reps []int
		p := i + k
		for p+4 <= len(s) && s[p] == ',' && isDigit(s[p+1]) && isDigit(s[p+2]) && isDigit(s[p+3]) {
			p += 4
			reps = append(reps, p)
		}
		for r := len(reps) - 1; r >= 0; r-- {
			e := reps[r]
			if e < len(s) && s[e] == '.' {
				if fd := digitsFrom(s, e+1); fd > 0 {
					for f := fd; f >= 1; f-- {
						out = append(out, e+1+f)
					}
				}
			}
			out = append(out, e)
		}
	}
	for k := d; k >= 1; k-- { // \d+\.\d+
		e := i + k
		if e < len(s) && s[e] == '.' {
			for f := digitsFrom(s, e+1); f >= 1; f-- {
				out = append(out, e+1+f)
			}
		}
	}
	for k := d; k >= 1; k-- { // \d+
		out = append(out, i+k)
	}
	return out
}

type figMatch struct {
	end           int
	cur, unit     bool
	commaOrPeriod bool
}

// figAt tries the figure pattern at byte i.
func figAt(s string, i int) (figMatch, bool) {
	if i > 0 && (isWord(s[i-1]) || s[i-1] == '.') {
		return figMatch{}, false
	}
	type start struct {
		at  int
		cur bool
	}
	var starts []start
	for _, c := range figCurrencies {
		if strings.HasPrefix(s[i:], c) {
			p := i + len(c)
			if p < len(s) && isWS(s[p]) {
				starts = append(starts, start{p + 1, true})
			}
			starts = append(starts, start{p, true})
		}
	}
	starts = append(starts, start{i, false})
	ahead := func(e int) bool { return e >= len(s) || !isWord(s[e]) }
	for _, st := range starts {
		for _, n := range numEnds(s, st.at) {
			num := s[st.at:n]
			cp := strings.ContainsAny(num, ",.")
			for _, sp := range []int{1, 0} { // (?:\s?(unit))? : with the group first, \s? greedy
				p := n
				if sp == 1 {
					if p < len(s) && isWS(s[p]) {
						p++
					} else {
						continue
					}
				}
				for _, u := range figUnits {
					if strings.HasPrefix(s[p:], u) && ahead(p+len(u)) {
						return figMatch{p + len(u), st.cur, true, cp}, true
					}
				}
			}
			if ahead(n) {
				return figMatch{n, st.cur, false, cp}, true
			}
		}
	}
	return figMatch{}, false
}

// Figures returns the numbers in text that state a quantity, in order of appearance.
func Figures(text string) []string {
	out := []string{}
	for i := 0; i < len(text); {
		m, ok := figAt(text, i)
		if !ok {
			i++
			continue
		}
		if m.cur || m.unit || m.commaOrPeriod {
			out = append(out, text[i:m.end])
		}
		i = m.end
	}
	return out
}

// ---------------------------------------------------------------- flag_unsourced

func viewText(view *Object) string {
	missing := view.GetOr("missing", "")
	if l, ok := missing.([]Value); ok {
		parts := make([]string, len(l))
		for i, x := range l {
			parts[i] = Str(x)
		}
		missing = strings.Join(parts, " ")
	}
	return strings.Join([]string{Str(view.GetOr("contribution", "")), Str(view.GetOr("devils_advocate", "")),
		Str(view.GetOr("done_so_far", "")), Str(missing)}, " ")
}

// FlagUnsourced returns the flags for one view (a JSON object), in order: every figure when the view cites
// no source (unsourced_figures), then every cited source id not in allowed (unknown_source; when allowed
// is empty every cited id is unknown).
func FlagUnsourced(view *Object, allowed Value) []Value {
	out := []Value{}
	who := view.GetOr("stakeholder", "")
	var cited []string
	if src := view.GetOr("sources", []Value{}); Truthy(src) {
		for _, s := range pyIter(src) {
			if t, ok := s.(string); ok {
				cited = append(cited, t)
			}
		}
	}
	if len(cited) == 0 {
		for _, f := range Figures(viewText(view)) {
			out = append(out, Obj("flag", "unsourced_figures", "figure", f, "stakeholder", who))
		}
	}
	allow := map[string]bool{}
	if Truthy(allowed) {
		for _, a := range pyIter(allowed) {
			if t, ok := a.(string); ok {
				allow[t] = true
			}
		}
	}
	for _, s := range cited {
		if !allow[s] {
			out = append(out, Obj("flag", "unknown_source", "source", s, "stakeholder", who))
		}
	}
	return out
}

// ---------------------------------------------------------------- generic steps, fallback, placeholders

// trimTail removes trailing [ \t\n\r\f\v]* and then trailing [.!?]* (the tail of the generic patterns).
func trimTail(s string) string {
	e := len(s)
	for e > 0 && isWS(s[e-1]) {
		e--
	}
	for e > 0 && (s[e-1] == '.' || s[e-1] == '!' || s[e-1] == '?') {
		e--
	}
	return s[:e]
}

// ^\s*advance the [\w/ -]+ with the decided approach[.!?]*\s*$  (case-insensitive)
func genericAdvance(s string) bool {
	t := trimTail(s[skipWS(s, 0):])
	const head, tail = "advance the ", " with the decided approach"
	if len(t) < len(head)+1+len(tail) || !hasCI(t, 0, head) || !hasCI(t, len(t)-len(tail), tail) {
		return false
	}
	mid := t[len(head) : len(t)-len(tail)]
	for i := 0; i < len(mid); i++ {
		if c := mid[i]; !(isWord(c) || c == '/' || c == ' ' || c == '-') {
			return false
		}
	}
	return true
}

// ^\s*[A-Za-z]+\s+[XYZ][.!?]*\s*$
func genericXYZ(s string) bool {
	i := skipWS(s, 0)
	j := i
	for j < len(s) && (s[j] >= 'a' && s[j] <= 'z' || s[j] >= 'A' && s[j] <= 'Z') {
		j++
	}
	if j == i {
		return false
	}
	k := skipWS(s, j)
	if k == j || k >= len(s) || (s[k] != 'X' && s[k] != 'Y' && s[k] != 'Z') {
		return false
	}
	return len(trimTail(s[k+1:])) == 0
}

var placeholderWords = []string{"tbd", "todo", "na", "n/a", "none", "proceed", "continue", "next step", "next steps"}

func genericWordTail(t string) bool {
	u := asciiLower(trimTail(t))
	if u == "" {
		return true
	}
	for _, w := range placeholderWords {
		if u == w {
			return true
		}
	}
	return false
}

// ^\s*(next steps?\s*:?\s*)?(tbd|todo|n/?a|none|proceed|continue|next steps?)?[.!?]*\s*$  (case-insensitive)
func genericWord(s string) bool {
	s = s[skipWS(s, 0):]
	if genericWordTail(s) {
		return true
	}
	if !hasCI(s, 0, "next step") {
		return false
	}
	p := len("next step")
	if hasCI(s, p, "s") {
		p++
	}
	p = skipWS(s, p)
	if p < len(s) && s[p] == ':' {
		p++
	}
	return genericWordTail(s[skipWS(s, p):])
}

// IsGeneric reports whether a next step or story title names nothing (fallback text included).
func IsGeneric(step Value) bool {
	s, ok := step.(string)
	if !ok || blankStr(s) {
		return true
	}
	return genericAdvance(s) || genericXYZ(s) || genericWord(s) || IsFallback(s)
}

var fallbackPhrases = []string{"synthesis failed", "falling back to the raw panel", "fell back to the raw panel",
	"fall back to the raw panel", "returned unparseable output", "unparseable output", "offline fallback",
	"(offline)", "fallback template"}

// IsFallback reports text a tool wrote when the model failed.
func IsFallback(text string) bool {
	low := asciiLower(text)
	for _, p := range fallbackPhrases {
		if strings.Contains(low, p) {
			return true
		}
	}
	return false
}

func isFallbackValue(v Value) bool {
	if !Truthy(v) {
		return false
	}
	return IsFallback(Str(v))
}

func wordEnd(s string, i int) bool { return i >= len(s) || !isWord(s[i]) }

// HasPlaceholderCode reports "TODO" stubs and `return {'ok': True}` bodies presented as code.
func HasPlaceholderCode(text string) bool {
	s := text
	quote := func(c byte) bool { return c == '\'' || c == '"' }
	for i := 0; i < len(s); i++ {
		switch lowerASCII(s[i]) {
		case '#': // #\s*TODO\b
			p := skipWS(s, i+1)
			if hasCI(s, p, "todo") && wordEnd(s, p+4) {
				return true
			}
		case '/': // //\s*TODO\b
			if i+1 < len(s) && s[i+1] == '/' {
				p := skipWS(s, i+2)
				if hasCI(s, p, "todo") && wordEnd(s, p+4) {
					return true
				}
			}
		case 'r':
			// return\s*\{\s*['"]ok['"]\s*:\s*True\s*\}
			if hasCI(s, i, "return") {
				p := skipWS(s, i+6)
				if p < len(s) && s[p] == '{' {
					p = skipWS(s, p+1)
					if p+4 <= len(s) && quote(s[p]) && hasCI(s, p+1, "ok") && quote(s[p+3]) {
						p = skipWS(s, p+4)
						if p < len(s) && s[p] == ':' {
							p = skipWS(s, p+1)
							if hasCI(s, p, "true") {
								p = skipWS(s, p+4)
								if p < len(s) && s[p] == '}' {
									return true
								}
							}
						}
					}
				}
			}
			if hasCI(s, i, "raise notimplementederror") {
				return true
			}
		case 'p': // \bpass\s*#\s*TODO
			if (i == 0 || !isWord(s[i-1])) && hasCI(s, i, "pass") {
				p := skipWS(s, i+4)
				if p < len(s) && s[p] == '#' && hasCI(s, skipWS(s, p+1), "todo") {
					return true
				}
			}
		}
	}
	return false
}

// SynthesisOK reports whether a merge (a JSON object) genuinely decided something: status full (or none
// given), a non-blank summary and decision, no fallback text, at least one resolved entry and at least one
// next step that is not generic.
func SynthesisOK(merge *Object) bool {
	if st, ok := merge.Get("synthesis_status"); ok {
		if s, isStr := st.(string); !isStr || s != "full" {
			return false
		}
	}
	summary, decision := merge.GetOr("merge_summary", ""), merge.GetOr("decision", "")
	if Blank(summary) || Blank(decision) {
		return false
	}
	resolved := merge.GetOr("resolved", nil)
	steps := merge.GetOr("next_steps", nil)
	var rs, ss []Value
	if Truthy(resolved) {
		rs = pyIter(resolved)
	}
	if Truthy(steps) {
		ss = pyIter(steps)
	}
	all := append(append([]Value{summary, decision}, rs...), ss...)
	for _, t := range all {
		if isFallbackValue(t) {
			return false
		}
	}
	if !Truthy(resolved) {
		return false
	}
	for _, s := range ss {
		if !IsGeneric(s) {
			return true
		}
	}
	return false
}

// ---------------------------------------------------------------- compliance and redaction

// Rule is a compliance rule: it fires when the lowercased text holds a word of every group.
type Rule struct {
	Name   string
	Groups [][]string
}

// DefaultRules are the keyword stubs of the reference (a private build plugs in a real policy).
var DefaultRules = []Rule{
	{"possible trading guidance on non-public information",
		[][]string{{"non-public", "nonpublic", "mnpi", "insider information"}, {"buy", "sell", "trade", "short", "position"}}},
	{"possible credential or secret in the output",
		[][]string{{"-----begin", "aws_secret_access_key", "api_key=", "password=", "ghp_", "github" + "_pat_", "pypi-ag"}}},
}

// ComplianceBlock returns the names of the rules that fire on text (nil rules: DefaultRules).
func ComplianceBlock(text string, rules []Rule) []string {
	if rules == nil {
		rules = DefaultRules
	}
	low := pyLower(text)
	out := []string{}
	for _, r := range rules {
		all := true
		for _, g := range r.Groups {
			anyWord := false
			for _, w := range g {
				if strings.Contains(low, w) {
					anyWord = true
					break
				}
			}
			if !anyWord {
				all = false
				break
			}
		}
		if all {
			out = append(out, r.Name)
		}
	}
	return out
}

var secretKeys = []string{"api_key", "api-key", "apikey", "access_token", "access-token", "accesstoken", "token",
	"secret", "password", "passwd", "signature", "sig", "key"}

// secretAt matches \b(api[_-]?key|...|key)(\s*[=:]\s*)([^\s&;,"']+) at i (case-insensitive) and returns the
// ends of the separator and of the value.
func secretAt(s string, i int) (int, int, bool) {
	if i > 0 && isWord(s[i-1]) || i >= len(s) || !isWord(s[i]) {
		return 0, 0, false
	}
	for _, k := range secretKeys {
		if !hasCI(s, i, k) {
			continue
		}
		g1 := i + len(k)
		p := skipWS(s, g1)
		if p >= len(s) || (s[p] != '=' && s[p] != ':') {
			continue
		}
		g2 := skipWS(s, p+1)
		e := g2
		for e < len(s) && !isWS(s[e]) && !strings.ContainsRune("&;,\"'", rune(s[e])) {
			e++
		}
		if e == g2 {
			continue
		}
		return g2, e, true
	}
	return 0, 0, false
}

func isTokenChar(c byte) bool {
	return c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || strings.IndexByte("._~+/=-", c) >= 0
}

// Redact makes error text safe to record: key=VALUE-style secrets and bearer tokens become ***.
func Redact(text string) string {
	var b strings.Builder
	s := text
	for i := 0; i < len(s); {
		if g2, e, ok := secretAt(s, i); ok {
			b.WriteString(s[i:g2])
			b.WriteString("***")
			i = e
			continue
		}
		b.WriteByte(s[i])
		i++
	}
	s = b.String()
	b.Reset()
	for i := 0; i < len(s); {
		if k, e, ok := bearerAt(s, i); ok {
			b.WriteString(s[i : i+len(k)])
			b.WriteString(" ***")
			i = e
			continue
		}
		b.WriteByte(s[i])
		i++
	}
	return b.String()
}

// bearerAt matches \b(bearer|basic)\s+[A-Za-z0-9._~+/=-]{8,} at i (case-insensitive).
func bearerAt(s string, i int) (string, int, bool) {
	if i > 0 && isWord(s[i-1]) {
		return "", 0, false
	}
	for _, k := range []string{"bearer", "basic"} {
		if !hasCI(s, i, k) {
			continue
		}
		p := skipWS(s, i+len(k))
		if p == i+len(k) {
			continue
		}
		e := p
		for e < len(s) && isTokenChar(s[e]) {
			e++
		}
		if e-p >= 8 {
			return k, e, true
		}
	}
	return "", 0, false
}
