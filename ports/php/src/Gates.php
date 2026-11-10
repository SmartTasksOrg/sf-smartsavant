<?php
// Canonical JSON and the quality gates: pure functions over JSON data (vectors: spec/conformance/
// canonical.json, gates.json, figures.json). Every pattern runs on UTF-8 bytes without the /u flag, so
// \d, letters and case folding are ASCII only, as in Python's re.ASCII; whitespace is spelled
// [\x09-\x0D\x20] ([ \t\n\r\f\v]); non-ASCII characters in a pattern are written as their UTF-8 bytes.
declare(strict_types=1);

namespace SmartTasks\SmartSavant;

/** Canonical JSON: keys ASCII and sorted, no whitespace, integers only, only " \ and controls escaped. */
final class Canonical
{
    private static ?array $map = null;

    private static function str(string $s): string
    {
        if (Py::hasLone($s) || !Py::validText($s)) {
            throw new \InvalidArgumentException('canonical JSON strings must be UTF-8 without unpaired surrogates');
        }
        if (self::$map === null) {
            self::$map = ['"' => '\\"', '\\' => '\\\\', "\x08" => '\\b', "\x0C" => '\\f', "\n" => '\\n', "\r" => '\\r',
                "\t" => '\\t'];
            for ($c = 0; $c < 0x20; $c++) self::$map[\chr($c)] ??= sprintf('\\u%04x', $c);
        }
        return '"' . strtr($s, self::$map) . '"';
    }

    /** The canonical JSON text. Floats, non-ASCII keys and other types throw \InvalidArgumentException. */
    public static function dumps(mixed $v): string
    {
        if ($v === null) return 'null';
        if ($v === true) return 'true';
        if ($v === false) return 'false';
        if (Py::isInt($v)) return (string) $v;
        if (is_string($v)) return self::str($v);
        if (is_array($v) && array_is_list($v)) return '[' . implode(',', array_map([self::class, 'dumps'], $v)) . ']';
        if (is_array($v)) $v = Obj::of($v);
        if ($v instanceof Obj) {
            $keys = $v->keys();
            foreach ($keys as $k) {
                if (preg_match('/[\x80-\xFF]/', $k) === 1) {
                    throw new \InvalidArgumentException('canonical JSON object keys must be ASCII strings');
                }
            }
            $parts = [];
            foreach (Py::sorted($keys) as $k) $parts[] = self::str($k) . ':' . self::dumps($v->get($k));
            return '{' . implode(',', $parts) . '}';
        }
        throw new \InvalidArgumentException('canonical JSON does not encode ' . get_debug_type($v) . ' (floats are not allowed)');
    }

    public static function sha256Hex(string $bytes): string { return hash('sha256', $bytes); }
}

final class Gates
{
    private const S = '[\x09-\x0D\x20]';
    private const UNIT = '%|percent|ms|\xC2\xB5s|us|ns|s|sec|secs|seconds?|min|mins|minutes?|h|hrs?|hours?|days?|weeks?|'
        . 'months?|years?|x|k|K|m|M|b|B|bn|mn|GB|MB|KB|TB|GiB|MiB|KiB|TiB|kpps|pps|rps|qps|tps|req/s|tokens?|users?|'
        . 'agents?|USD|EUR|GBP';
    private const W = '[A-Za-z0-9_]';

    private static function fig(): string
    {
        return '~(?<![A-Za-z0-9_.])(?:(?<cur>\xC2\xA3|\$|\xE2\x82\xAC)' . self::S . '?)?'
            . '(?<num>[0-9]{1,3}(?:,[0-9]{3})+(?:\.[0-9]+)?|[0-9]+\.[0-9]+|[0-9]+)'
            . '(?:' . self::S . '?(?<unit>' . self::UNIT . '))?(?!' . self::W . ')~';
    }

    /** Runs of [ \t\n\r\f\v] to one space, trimmed. */
    public static function squash(mixed $text): string
    {
        $t = Py::truthy($text) ? Py::str($text) : '';
        return implode(' ', preg_split('/[\x09-\x0D\x20]+/', $t, -1, PREG_SPLIT_NO_EMPTY));
    }

    /** True for a non-string, or a string empty or made only of [ \t\n\r\f\v]. */
    public static function blank(mixed $text): bool
    {
        return !is_string($text) || preg_match('/^[\x09-\x0D\x20]*\z/', $text) === 1;
    }

    /** Figures in order of appearance, as written. @return list<string> */
    public static function figures(mixed $text): array
    {
        $t = Py::truthy($text) ? Py::str($text) : '';
        preg_match_all(self::fig(), $t, $ms, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL);
        $out = [];
        foreach ($ms as $m) {
            $num = $m['num'];
            if (($m['cur'] ?? null) !== null || ($m['unit'] ?? null) !== null || strpbrk($num, ',.') !== false) {
                $out[] = trim($m[0], " \t\n\r\x0B\x0C");
            }
        }
        return $out;
    }

    private static function viewText(Obj $view): string
    {
        $missing = $view->get('missing', '');
        if (Py::isList($missing)) $missing = implode(' ', array_map([Py::class, 'str'], $missing));
        return implode(' ', array_map([Py::class, 'str'], [$view->get('contribution', ''), $view->get('devils_advocate', ''),
            $view->get('done_so_far', ''), $missing]));
    }

    /** unsourced_figures flags when the view cites no source, then unknown_source per cited id not allowed. */
    public static function flagUnsourced(Obj $view, mixed $allowedSources): array
    {
        $out = [];
        $who = $view->get('stakeholder', '');
        $src = $view->get('sources', []);
        $cited = array_values(array_filter(Py::truthy($src) ? Py::iter($src) : [], 'is_string'));
        if (!$cited) {
            foreach (self::figures(self::viewText($view)) as $f) {
                $out[] = Obj::of(['flag' => 'unsourced_figures', 'figure' => $f, 'stakeholder' => $who]);
            }
        }
        $allowed = Py::truthy($allowedSources) ? Py::iter($allowedSources) : [];
        foreach ($cited as $s) {
            if (!in_array($s, $allowed, true)) $out[] = Obj::of(['flag' => 'unknown_source', 'source' => $s, 'stakeholder' => $who]);
        }
        return $out;
    }

    private static function generic(): array
    {
        $s = self::S;
        return [
            "~^$s*advance the [A-Za-z0-9_/ -]+ with the decided approach[.!?]*$s*$~i",
            "~^$s*[A-Za-z]+$s+[XYZ][.!?]*$s*$~",
            "~^$s*(next steps?$s*:?$s*)?(tbd|todo|n/?a|none|proceed|continue|next steps?)?[.!?]*$s*$~i",
        ];
    }

    /** A next step or story title that names no real component or decision (fallback text included). */
    public static function isGeneric(mixed $step): bool
    {
        if (self::blank($step)) return true;
        foreach (self::generic() as $p) if (preg_match($p, $step) === 1) return true;
        return self::isFallback($step);
    }

    private static function text(mixed $t): string { return Py::truthy($t) ? Py::str($t) : ''; }

    public static function isFallback(mixed $text): bool
    {
        return preg_match('~synthesis failed|(falling|fell|fall) back to the raw panel|returned unparseable output|'
            . 'unparseable output|offline fallback|\(offline\)|fallback template~i', self::text($text)) === 1;
    }

    public static function hasPlaceholderCode(mixed $text): bool
    {
        $s = self::S;
        $w = self::W;
        return preg_match("~#$s*TODO(?!$w)|//$s*TODO(?!$w)|return$s*\\{{$s}*['\"]ok['\"]$s*:$s*True$s*\\}|"
            . "raise NotImplementedError|(?<!$w)pass$s*#$s*TODO~i", self::text($text)) === 1;
    }

    /** A merge that genuinely decided something. */
    public static function synthesisOk(Obj $merge): bool
    {
        if ($merge->get('synthesis_status', 'full') !== 'full') return false;
        $summary = $merge->get('merge_summary', '');
        $decision = $merge->get('decision', '');
        if (self::blank($summary) || self::blank($decision)) return false;
        $resolved = $merge->get('resolved');
        $steps = $merge->get('next_steps');
        $rs = Py::truthy($resolved) ? Py::iter($resolved) : [];
        $ss = Py::truthy($steps) ? Py::iter($steps) : [];
        foreach ([$summary, $decision, ...$rs, ...$ss] as $t) if (self::isFallback($t)) return false;
        if (!$rs) return false;
        foreach ($ss as $s) if (!self::isGeneric($s)) return true;
        return false;
    }

    public static function defaultRules(): array
    {
        return [
            ['possible trading guidance on non-public information',
                [['non-public', 'nonpublic', 'mnpi', 'insider information'], ['buy', 'sell', 'trade', 'short', 'position']]],
            ['possible credential or secret in the output',
                [['-----begin', 'aws_secret_access_key', 'api_key=', 'password=', 'ghp_', 'github' . '_pat_', 'pypi-ag']]],
        ];
    }

    /** Reasons to block a release of this text ([] when no rule fired). Keyword matching only.
     *  $rules: list of [name, list of word groups]; null means the defaults. */
    public static function complianceBlock(mixed $text, ?array $rules = null): array
    {
        $low = Py::lower(self::text($text));
        $out = [];
        foreach ($rules ?? self::defaultRules() as [$name, $groups]) {
            $all = true;
            foreach ($groups as $g) {
                $any = false;
                foreach ($g as $w) if (str_contains($low, $w)) { $any = true; break; }
                if (!$any) { $all = false; break; }
            }
            if ($all) $out[] = $name;
        }
        return $out;
    }

    /** Error text made safe to record: key=VALUE-style secrets and bearer tokens replaced by ***. */
    public static function redact(mixed $text): string
    {
        $s = self::S;
        $w = self::W;
        $t = preg_replace_callback("~(?<!$w)(api[_-]?key|access[_-]?token|token|secret|password|passwd|signature|sig|key)"
            . "($s*[=:]$s*)([^\\x09-\\x0D\\x20&;,\"']+)~i", fn ($m) => $m[1] . $m[2] . '***', self::text($text));
        return preg_replace_callback("~(?<!$w)(bearer|basic)$s+[A-Za-z0-9._\\~+/=-]{8,}~i", fn ($m) => $m[1] . ' ***', $t);
    }
}
