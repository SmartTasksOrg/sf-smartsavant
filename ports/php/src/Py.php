<?php
// The data representation of this port and the Python behaviour the reference relies on.
//
// JSON data is: null, bool, int, float, string, a list (a PHP array with keys 0..n-1) or an Obj (an ordered
// map with string keys; a PHP array would turn the key "1" into the integer 1 and lose {} versus []).
// Strings are UTF-8 bytes. An unpaired surrogate (JSON "\ud800") is kept as its 3-byte generalised UTF-8
// form (ED A0 80, "WTF-8"), so it can be detected as Python detects it. Lengths and slices count code points.
declare(strict_types=1);

namespace SmartTasks\SmartSavant;

/** An ordered JSON object with string keys. Keys such as "1", "", "__proto__" or "\0x" are plain data. */
final class Obj implements \IteratorAggregate, \Countable
{
    /** @var array<string, mixed> each key prefixed with "k" so that PHP never turns it into an integer */
    private array $m = [];

    /** An Obj from a PHP array written in code (keys taken as strings). */
    public static function of(array $a = []): self
    {
        $o = new self();
        foreach ($a as $k => $v) $o->m['k' . $k] = $v;
        return $o;
    }

    public function get(string $k, mixed $default = null): mixed
    {
        return array_key_exists('k' . $k, $this->m) ? $this->m['k' . $k] : $default;
    }

    public function has(string $k): bool { return array_key_exists('k' . $k, $this->m); }

    /** Sets $k (a new key goes last; an existing key keeps its place, as in a Python dict). */
    public function set(string $k, mixed $v): self
    {
        $this->m['k' . $k] = $v;
        return $this;
    }

    public function del(string $k): void { unset($this->m['k' . $k]); }

    /** @return list<string> */
    public function keys(): array
    {
        $out = [];
        foreach ($this->m as $k => $_) $out[] = substr((string) $k, 1);
        return $out;
    }

    /** @return list<mixed> */
    public function values(): array { return array_values($this->m); }

    public function count(): int { return count($this->m); }

    public function getIterator(): \Generator
    {
        foreach ($this->m as $k => $v) yield substr((string) $k, 1) => $v;
    }

    public function __get(string $k): mixed { return $this->get($k); }
    public function __set(string $k, mixed $v): void { $this->set($k, $v); }
    public function __isset(string $k): bool { return $this->has($k); }
}

/** An integer outside PHP's signed 64-bit range, kept exactly as its decimal digits (a Python int). */
final class BigInt
{
    public function __construct(public readonly string $digits) {}

    public function __toString(): string { return $this->digits; }
}

/** Python behaviour: code-point lengths and slices, str(), repr(), truthiness, sorting, json.dumps. */
final class Py
{
    private const LONE = '/\xED[\xA0-\xBF][\x80-\xBF]/';

    public static function isList(mixed $v): bool { return is_array($v) && array_is_list($v); }

    /** A Python int: a PHP int or a BigInt. */
    public static function isInt(mixed $v): bool { return is_int($v) || $v instanceof BigInt; }

    /** len(s) in code points. */
    public static function len(string $s): int
    {
        return strlen($s) - preg_match_all('/[\x80-\xBF]/', $s);
    }

    /** s[:n] in code points. */
    public static function head(string $s, int $n): string
    {
        if (strlen($s) <= $n) return $s;
        $seen = 0;
        $len = strlen($s);
        for ($i = 0; $i < $len; $i++) {
            $b = ord($s[$i]);
            if ($b < 0x80 || $b >= 0xC0) {
                if ($seen === $n) return substr($s, 0, $i);
                $seen++;
            }
        }
        return $s;
    }

    /** The code points of $s, each as its UTF-8 bytes. @return list<string> */
    public static function chars(string $s): array
    {
        preg_match_all('/[\xC0-\xFF][\x80-\xBF]*|[\x00-\xBF]/', $s, $m);
        return $m[0];
    }

    /** The code point of one character (as chars() returns it). */
    public static function ord(string $c): int
    {
        $b = ord($c[0]);
        if ($b < 0x80) return $b;
        $n = $b >= 0xF0 ? 3 : ($b >= 0xE0 ? 2 : 1);
        $cp = $b & (0x3F >> $n);
        for ($i = 1; $i <= $n && $i < strlen($c); $i++) $cp = ($cp << 6) | (ord($c[$i]) & 0x3F);
        return $cp;
    }

    /** chr(cp) as UTF-8 (a surrogate as its 3-byte form). */
    public static function chr(int $cp): string
    {
        if ($cp < 0x80) return \chr($cp);
        if ($cp < 0x800) return \chr(0xC0 | ($cp >> 6)) . \chr(0x80 | ($cp & 0x3F));
        if ($cp < 0x10000) {
            return \chr(0xE0 | ($cp >> 12)) . \chr(0x80 | (($cp >> 6) & 0x3F)) . \chr(0x80 | ($cp & 0x3F));
        }
        return \chr(0xF0 | ($cp >> 18)) . \chr(0x80 | (($cp >> 12) & 0x3F)) . \chr(0x80 | (($cp >> 6) & 0x3F))
            . \chr(0x80 | ($cp & 0x3F));
    }

    /** True when $s holds an unpaired surrogate (U+D800-U+DFFF). */
    public static function hasLone(string $s): bool { return preg_match('/\xED[\xA0-\xBF]/', $s) === 1; }

    public static function replaceLone(string $s, string $by): string { return preg_replace(self::LONE, $by, $s); }

    /** UTF-8 text, surrogate code points allowed (what a Python str can hold). */
    public static function validText(string $s): bool
    {
        return preg_match('//u', preg_replace(self::LONE, 'a', $s)) === 1;
    }

    /** s.encode('utf-8', 'replace').decode(): lone surrogates become '?'; other invalid bytes too. */
    public static function scrub(string $s): string
    {
        $s = self::replaceLone($s, '?');
        return preg_match('//u', $s) === 1 ? $s : mb_scrub($s, 'UTF-8');
    }

    /** str.lower() (full Unicode lower-casing, as mbstring does it). */
    public static function lower(string $s): string { return mb_strtolower(self::replaceLone($s, '?'), 'UTF-8'); }

    /** Python ordering of str: by code point, which is the byte order of UTF-8. */
    public static function sorted(array $a): array
    {
        $a = array_values($a);
        usort($a, 'strcmp');
        return $a;
    }

    /** bool(v). */
    public static function truthy(mixed $v): bool
    {
        if ($v === null || $v === false || $v === 0 || $v === '' || $v === []) return false;
        if (is_float($v)) return $v != 0.0 || is_nan($v);
        if ($v instanceof Obj) return count($v) > 0;
        return true;
    }

    /** Iterating a value as Python does: a str gives its characters, a dict its keys. */
    public static function iter(mixed $v): array
    {
        if (is_string($v)) return self::chars($v);
        if (is_array($v)) return array_values($v);
        if ($v instanceof Obj) return $v->keys();
        return [];
    }

    /** repr(float). */
    public static function floatRepr(float $f): string
    {
        if (is_nan($f)) return 'nan';
        if (is_infinite($f)) return $f > 0 ? 'inf' : '-inf';
        if ($f == 0.0) return (fdiv(1.0, $f) < 0 ? '-' : '') . '0.0';
        // the shortest digits that read back as the same double
        for ($p = 0; $p < 17; $p++) {
            $e = sprintf('%.' . $p . 'e', $f);
            if ((float) $e === $f) break;
        }
        preg_match('/^(-?)(\d)(?:\.(\d+))?e([+-]\d+)$/', $e, $m);
        $sign = $m[1];
        $digits = rtrim($m[2] . ($m[3] ?? ''), '0');
        if ($digits === '') $digits = '0';
        $exp = (int) $m[4];
        if ($exp >= -4 && $exp < 16) {
            $point = $exp + 1;
            if ($point <= 0) return $sign . '0.' . str_repeat('0', -$point) . $digits;
            if ($point >= strlen($digits)) return $sign . $digits . str_repeat('0', $point - strlen($digits)) . '.0';
            return $sign . substr($digits, 0, $point) . '.' . substr($digits, $point);
        }
        $mant = $digits[0] . (strlen($digits) > 1 ? '.' . substr($digits, 1) : '');
        return $sign . $mant . 'e' . ($exp < 0 ? '-' : '+') . str_pad((string) abs($exp), 2, '0', STR_PAD_LEFT);
    }

    /** repr(str). Printability of non-ASCII characters follows PCRE's Unicode tables. */
    public static function reprStr(string $s): string
    {
        $q = (str_contains($s, "'") && !str_contains($s, '"')) ? '"' : "'";
        $out = $q;
        foreach (self::chars($s) as $ch) {
            $c = self::ord($ch);
            if ($ch === $q || $ch === '\\') $out .= '\\' . $ch;
            elseif ($ch === "\n") $out .= '\\n';
            elseif ($ch === "\r") $out .= '\\r';
            elseif ($ch === "\t") $out .= '\\t';
            elseif ($c < 0x20 || ($c >= 0x7F && $c <= 0xA0)) $out .= sprintf('\\x%02x', $c);
            elseif ($c >= 0xD800 && $c <= 0xDFFF) $out .= sprintf('\\u%04x', $c);
            elseif ($c > 0x7F && preg_match('/^[\p{Cc}\p{Cf}\p{Co}\p{Cn}\p{Zl}\p{Zp}\p{Zs}]$/u', $ch) === 1) {
                $out .= $c < 0x100 ? sprintf('\\x%02x', $c) : ($c < 0x10000 ? sprintf('\\u%04x', $c) : sprintf('\\U%08x', $c));
            } else $out .= $ch;
        }
        return $out . $q;
    }

    public static function repr(mixed $v): string
    {
        if ($v === null) return 'None';
        if ($v === true) return 'True';
        if ($v === false) return 'False';
        if (self::isInt($v)) return (string) $v;
        if (is_float($v)) return self::floatRepr($v);
        if (is_string($v)) return self::reprStr($v);
        if (is_array($v)) {
            if (array_is_list($v)) return '[' . implode(', ', array_map([self::class, 'repr'], $v)) . ']';
            $v = Obj::of($v);
        }
        if ($v instanceof Obj) {
            $parts = [];
            foreach ($v as $k => $x) $parts[] = self::reprStr($k) . ': ' . self::repr($x);
            return '{' . implode(', ', $parts) . '}';
        }
        return get_debug_type($v);
    }

    /** str(v). */
    public static function str(mixed $v): string { return is_string($v) ? $v : self::repr($v); }

    /** A deep copy of JSON data (copy.deepcopy for what adapters see). */
    public static function copy(mixed $v): mixed
    {
        if ($v instanceof Obj) {
            $o = new Obj();
            foreach ($v as $k => $x) $o->set($k, self::copy($x));
            return $o;
        }
        if (is_array($v)) return array_map([self::class, 'copy'], $v);
        return $v;
    }

    /** JSON value equality as Python compares the vectors (key order ignored); true is not 1. */
    public static function same(mixed $a, mixed $b): bool
    {
        if (is_bool($a) || is_bool($b) || $a === null || $b === null) return $a === $b;
        if ($a instanceof BigInt || $b instanceof BigInt) {
            return $a instanceof BigInt && $b instanceof BigInt && $a->digits === $b->digits;
        }
        if ((is_int($a) || is_float($a)) && (is_int($b) || is_float($b))) return $a == $b;
        if (is_string($a) || is_string($b)) return $a === $b;
        if (is_array($a) && is_array($b)) {
            if (count($a) !== count($b)) return false;
            foreach (array_keys($a) as $i) if (!array_key_exists($i, $b) || !self::same($a[$i], $b[$i])) return false;
            return true;
        }
        if ($a instanceof Obj && $b instanceof Obj) {
            if (count($a) !== count($b)) return false;
            foreach ($a as $k => $x) if (!$b->has($k) || !self::same($x, $b->get($k))) return false;
            return true;
        }
        return false;
    }
}

/** JSON text in and out: a strict reader (as Python's json.loads) and Python's json.dumps formatting. */
final class Json
{
    /**
     * Parse JSON text (UTF-8) into Obj / list / scalars. Integers stay int, numbers with a fraction or an
     * exponent are float, NaN / Infinity are accepted as by Python, an unpaired \ud800-style escape is
     * kept (see the file header). Integers outside the 64-bit range become BigInt. Throws \JsonException.
     */
    public static function decode(string $text): mixed
    {
        if (preg_match('//u', $text) !== 1) throw new \JsonException('not UTF-8 text');
        return (new JsonReader($text))->parse();
    }

    private static ?array $esc = null;

    private static function str(string $s, bool $ensureAscii): string
    {
        if (self::$esc === null) {
            self::$esc = ['"' => '\\"', '\\' => '\\\\', "\n" => '\\n', "\r" => '\\r', "\t" => '\\t', "\x08" => '\\b',
                "\x0C" => '\\f'];
            for ($c = 0; $c < 0x20; $c++) self::$esc[\chr($c)] ??= sprintf('\\u%04x', $c);
        }
        $s = strtr($s, self::$esc);
        if ($ensureAscii && preg_match('/[\x80-\xFF]/', $s) === 1) {
            $out = '';
            foreach (Py::chars($s) as $ch) {
                $c = Py::ord($ch);
                if ($c < 0x80) $out .= $ch;
                elseif ($c < 0x10000) $out .= sprintf('\\u%04x', $c);
                else {
                    $c -= 0x10000;
                    $out .= sprintf('\\u%04x\\u%04x', 0xD800 | ($c >> 10), 0xDC00 | ($c & 0x3FF));
                }
            }
            $s = $out;
        }
        return '"' . $s . '"';
    }

    /** json.dumps(v, ensure_ascii=$ensureAscii, sort_keys=$sortKeys) with the ", " and ": " separators. */
    public static function py(mixed $v, bool $sortKeys = false, bool $ensureAscii = false): string
    {
        if ($v === null) return 'null';
        if ($v === true) return 'true';
        if ($v === false) return 'false';
        if (Py::isInt($v)) return (string) $v;
        if (is_float($v)) {
            if (is_nan($v)) return 'NaN';
            if (is_infinite($v)) return $v > 0 ? 'Infinity' : '-Infinity';
            return Py::floatRepr($v);
        }
        if (is_string($v)) return self::str($v, $ensureAscii);
        if (is_array($v) && array_is_list($v)) {
            return '[' . implode(', ', array_map(fn ($x) => self::py($x, $sortKeys, $ensureAscii), $v)) . ']';
        }
        if (is_array($v)) $v = Obj::of($v);
        if ($v instanceof Obj) {
            $keys = $sortKeys ? Py::sorted($v->keys()) : $v->keys();
            $parts = [];
            foreach ($keys as $k) $parts[] = self::str($k, $ensureAscii) . ': ' . self::py($v->get($k), $sortKeys, $ensureAscii);
            return '{' . implode(', ', $parts) . '}';
        }
        throw new \InvalidArgumentException('Object of type ' . get_debug_type($v) . ' is not JSON serializable');
    }
}

/** @internal the reader behind Json::decode */
final class JsonReader
{
    private const MAX_DEPTH = 5000;
    private int $i = 0;
    private int $n;
    private int $depth = 0;
    private static ?string $stop = null;

    public function __construct(private string $s)
    {
        $this->n = strlen($s);
        if (self::$stop === null) {
            self::$stop = "\"\\";
            for ($c = 0; $c < 0x20; $c++) self::$stop .= \chr($c);
        }
    }

    public function parse(): mixed
    {
        $this->ws();
        $v = $this->value();
        $this->ws();
        if ($this->i !== $this->n) $this->fail('extra data');
        return $v;
    }

    private function fail(string $m): never { throw new \JsonException("$m at byte {$this->i}"); }

    private function ws(): void { $this->i += strspn($this->s, " \t\n\r", $this->i); }

    private function lit(string $w): bool
    {
        if (substr($this->s, $this->i, strlen($w)) !== $w) return false;
        $this->i += strlen($w);
        return true;
    }

    private function value(): mixed
    {
        if ($this->i >= $this->n) $this->fail('expecting value');
        $c = $this->s[$this->i];
        if ($c === '{' || $c === '[') {
            if (++$this->depth > self::MAX_DEPTH) $this->fail('nested too deeply');
            $v = $c === '{' ? $this->object() : $this->array();
            $this->depth--;
            return $v;
        }
        if ($c === '"') return $this->string();
        if ($this->lit('true')) return true;
        if ($this->lit('false')) return false;
        if ($this->lit('null')) return null;
        if ($this->lit('NaN')) return NAN;
        if ($this->lit('Infinity')) return INF;
        if ($this->lit('-Infinity')) return -INF;
        if (preg_match('/-?(?:0|[1-9][0-9]*)(\.[0-9]+)?([eE][-+]?[0-9]+)?/A', $this->s, $m, 0, $this->i) === 1) {
            $this->i += strlen($m[0]);
            $t = $m[0];
            if (($m[1] ?? '') === '' && ($m[2] ?? '') === '') {
                $iv = intval($t);
                if ((string) $iv === $t || $t === '-0') return $iv;
                return new BigInt($t);
            }
            return (float) $t;
        }
        $this->fail('expecting value');
    }

    private function object(): Obj
    {
        $o = new Obj();
        $this->i++;
        $this->ws();
        if ($this->i < $this->n && $this->s[$this->i] === '}') {
            $this->i++;
            return $o;
        }
        while (true) {
            $this->ws();
            if ($this->i >= $this->n || $this->s[$this->i] !== '"') $this->fail('expecting property name');
            $k = $this->string();
            $this->ws();
            if ($this->i >= $this->n || $this->s[$this->i] !== ':') $this->fail("expecting ':'");
            $this->i++;
            $this->ws();
            $o->set($k, $this->value());
            $this->ws();
            $c = $this->s[$this->i] ?? '';
            $this->i++;
            if ($c === '}') return $o;
            if ($c !== ',') $this->fail("expecting ',' or '}'");
        }
    }

    private function array(): array
    {
        $a = [];
        $this->i++;
        $this->ws();
        if ($this->i < $this->n && $this->s[$this->i] === ']') {
            $this->i++;
            return $a;
        }
        while (true) {
            $this->ws();
            $a[] = $this->value();
            $this->ws();
            $c = $this->s[$this->i] ?? '';
            $this->i++;
            if ($c === ']') return $a;
            if ($c !== ',') $this->fail("expecting ',' or ']'");
        }
    }

    private function hex4(int $at): ?int
    {
        $h = substr($this->s, $at, 4);
        return strlen($h) === 4 && ctype_xdigit($h) ? hexdec($h) : null;
    }

    private function string(): string
    {
        $this->i++;
        $out = '';
        while (true) {
            $k = strcspn($this->s, self::$stop, $this->i);
            $out .= substr($this->s, $this->i, $k);
            $this->i += $k;
            if ($this->i >= $this->n) $this->fail('unterminated string');
            $c = $this->s[$this->i];
            if ($c === '"') {
                $this->i++;
                return $out;
            }
            if ($c !== '\\') $this->fail('invalid control character');
            $e = $this->s[$this->i + 1] ?? '';
            $this->i += 2;
            switch ($e) {
                case '"': $out .= '"'; break;
                case '\\': $out .= '\\'; break;
                case '/': $out .= '/'; break;
                case 'b': $out .= "\x08"; break;
                case 'f': $out .= "\x0C"; break;
                case 'n': $out .= "\n"; break;
                case 'r': $out .= "\r"; break;
                case 't': $out .= "\t"; break;
                case 'u':
                    $cp = $this->hex4($this->i);
                    if ($cp === null) $this->fail('invalid \\uXXXX escape');
                    $this->i += 4;
                    if ($cp >= 0xD800 && $cp <= 0xDBFF && substr($this->s, $this->i, 2) === '\\u') {
                        $lo = $this->hex4($this->i + 2);
                        if ($lo !== null && $lo >= 0xDC00 && $lo <= 0xDFFF) {
                            $cp = 0x10000 + (($cp - 0xD800) << 10) + ($lo - 0xDC00);
                            $this->i += 6;
                        }
                    }
                    $out .= Py::chr($cp);
                    break;
                default:
                    $this->fail('invalid escape');
            }
        }
    }
}
