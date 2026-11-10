<?php
// Validation of what an adapter returns, and of council files. The error strings and their order are part
// of the contract (spec/conformance/schema.json).
declare(strict_types=1);

namespace SmartTasks\SmartSavant;

class NotPlainData extends \InvalidArgumentException
{
}

final class Schema
{
    public const MAX_STR = 8000;
    public const MAX_DELTA = 20000;
    public const MAX_ITEMS = 20;
    public const MAX_ITEM = 2000;
    public const MAX_DEPTH = 32;
    public const MAX_TOTAL = 1000000;
    public const SOURCE_ID = '~^[A-Za-z0-9._:/#@-]{1,200}\z~';
    public const W5H1_KEYS = ['who', 'what', 'where', 'when', 'why', 'how'];
    public const VIEW_KEYS = ['w5h1', 'done_so_far', 'missing', 'devils_advocate', 'contribution', 'sources'];
    public const MERGE_KEYS = ['merge_summary', 'decision', 'resolved', 'open_issues', 'next_steps', 'reversals', 'spec_delta',
        'loop_recommended'];

    /**
     * A copy of an adapter answer made only of null, bool, int, float, string, lists and Obj, built before
     * anything else so that what is validated is exactly what is used (schema.plain in Python).
     * Also accepted, and converted to Obj: PHP arrays that are not lists (integer keys become strings) and
     * \stdClass. Refused with NotPlainData: other objects and resources, strings that are not UTF-8 text,
     * nesting deeper than 32 and more than 1,000,000 code points of text (keys included).
     */
    public static function plain(mixed $obj): mixed
    {
        $budget = self::MAX_TOTAL;
        $text = function (string $s) use (&$budget): string {
            if (!Py::validText($s)) throw new NotPlainData('strings must be UTF-8 text');
            $budget -= Py::len($s);
            return $s;
        };
        $go = function (mixed $x, int $depth) use (&$go, &$budget, $text): mixed {
            if ($depth > self::MAX_DEPTH) throw new NotPlainData('nested deeper than 32 levels');
            if ($x === null || is_bool($x) || Py::isInt($x) || is_float($x)) return $x;
            if (is_string($x)) {
                $text($x);
                if ($budget < 0) throw new NotPlainData('more than 1,000,000 characters of text');
                return $x;
            }
            if (is_array($x) && array_is_list($x)) {
                $out = [];
                foreach ($x as $v) $out[] = $go($v, $depth + 1);
                return $out;
            }
            if (is_array($x) || $x instanceof Obj || (is_object($x) && get_class($x) === 'stdClass')) {
                $out = new Obj();
                foreach ($x as $k => $v) {
                    $k = $text((string) $k);
                    $out->set($k, $go($v, $depth + 1));
                }
                if ($budget < 0) throw new NotPlainData('more than 1,000,000 characters of text');
                return $out;
            }
            throw new NotPlainData(get_debug_type($x) . ' is not JSON data');
        };
        return $go($obj, 0);
    }

    /** An unknown key as shown in an error: control characters dropped, unpaired surrogates as U+FFFD, then at
     *  most 40 code points. */
    private static function showKey(string $k): string
    {
        $k = preg_replace('/[\x00-\x1F\x7F]|\xC2[\x80-\x9F]/', '', $k);
        $k = Py::replaceLone($k, "\u{FFFD}");               // error text is recorded: it must stay encodable
        return Py::head($k, 40) . (Py::len($k) > 40 ? '...' : '');
    }

    private static function blank(string $s): bool { return preg_match('/^[\x09-\x0D\x20]*\z/', $s) === 1; }

    private static function checkStr(array &$errs, string $where, mixed $v, int $limit = self::MAX_STR, bool $nonempty = false): void
    {
        if (!is_string($v)) $errs[] = "$where must be a string";
        elseif (Py::len($v) > $limit) $errs[] = "$where is longer than $limit characters";
        elseif (str_contains($v, "\0")) $errs[] = "$where contains a NUL character";
        elseif (Py::hasLone($v)) $errs[] = "$where contains an unpaired surrogate";
        elseif ($nonempty && self::blank($v)) $errs[] = "$where must not be empty";
    }

    private static function checkList(array &$errs, string $where, mixed $v, ?string $pattern = null): void
    {
        if (!Py::isList($v)) {
            $errs[] = "$where must be a list of strings";
            return;
        }
        if (count($v) > self::MAX_ITEMS) $errs[] = "$where has more than " . self::MAX_ITEMS . ' items';
        foreach (array_slice($v, 0, self::MAX_ITEMS + 1) as $i => $x) {
            if (!is_string($x) || self::blank($x)) $errs[] = "{$where}[$i] must be a non-empty string";
            elseif (Py::len($x) > self::MAX_ITEM) $errs[] = "{$where}[$i] is longer than " . self::MAX_ITEM . ' characters';
            elseif ($pattern !== null && preg_match($pattern, $x) !== 1) $errs[] = "{$where}[$i] is not a valid source id";
            elseif (str_contains($x, "\0")) $errs[] = "{$where}[$i] contains a NUL character";
            elseif (Py::hasLone($x)) $errs[] = "{$where}[$i] contains an unpaired surrogate";
        }
    }

    private static function sameKeys(Obj $o, array $keys): bool
    {
        if (count($o) !== count($keys)) return false;
        foreach ($keys as $k) if (!$o->has($k)) return false;
        return true;
    }

    private static function keyErrors(array &$errs, Obj $o, array $allowed, array $required): bool
    {
        $extra = Py::sorted(array_filter($o->keys(), fn ($k) => !in_array($k, $allowed, true)));
        if ($extra) {
            $errs[] = 'unknown keys (' . count($extra) . '): '
                . implode(', ', array_map([self::class, 'showKey'], array_slice($extra, 0, 5)));
        }
        $missing = Py::sorted(array_filter($required, fn ($k) => !$o->has($k)));
        if ($missing) {
            $errs[] = 'missing keys: ' . implode(', ', $missing);
            return false;
        }
        return true;
    }

    /** [] when $v is a valid view; otherwise the reasons, first problem first. */
    public static function checkView(mixed $v): array
    {
        if (!$v instanceof Obj) return ['a view must be a JSON object'];
        $errs = [];
        if (!self::keyErrors($errs, $v, self::VIEW_KEYS, self::VIEW_KEYS)) return $errs;
        $w = $v->get('w5h1');
        if (!$w instanceof Obj || !self::sameKeys($w, self::W5H1_KEYS)) {
            $errs[] = 'w5h1 must be an object with exactly who, what, where, when, why, how';
        } else {
            foreach (self::W5H1_KEYS as $k) self::checkStr($errs, "w5h1.$k", $w->get($k));
        }
        self::checkStr($errs, 'done_so_far', $v->get('done_so_far'));
        if (is_string($v->get('missing'))) self::checkStr($errs, 'missing', $v->get('missing'));
        else self::checkList($errs, 'missing', $v->get('missing'));
        self::checkStr($errs, 'devils_advocate', $v->get('devils_advocate'), self::MAX_STR, true);
        self::checkStr($errs, 'contribution', $v->get('contribution'));
        self::checkList($errs, 'sources', $v->get('sources'), self::SOURCE_ID);
        return $errs;
    }

    /** [] when $m is a valid merge; otherwise the reasons, first problem first. */
    public static function checkMerge(mixed $m): array
    {
        if (!$m instanceof Obj) return ['a merge must be a JSON object'];
        $errs = [];
        $required = array_values(array_filter(self::MERGE_KEYS, fn ($k) => $k !== 'reversals'));
        if (!self::keyErrors($errs, $m, self::MERGE_KEYS, $required)) return $errs;
        self::checkStr($errs, 'merge_summary', $m->get('merge_summary'), self::MAX_STR, true);
        self::checkStr($errs, 'decision', $m->get('decision'));
        foreach (['resolved', 'open_issues', 'next_steps'] as $k) self::checkList($errs, $k, $m->get($k));
        if ($m->has('reversals')) self::checkList($errs, 'reversals', $m->get('reversals'));
        self::checkStr($errs, 'spec_delta', $m->get('spec_delta'), self::MAX_DELTA);
        if (!is_bool($m->get('loop_recommended'))) $errs[] = 'loop_recommended must be true or false';
        return $errs;
    }

    /** A valid view with missing always a list (a shallow copy). */
    public static function normalizeView(Obj $v): Obj
    {
        $out = Py::copy($v);
        $m = $out->get('missing');
        if (is_string($m)) $out->set('missing', self::blank($m) ? [] : [$m]);
        return $out;
    }

    // ------------------------------------------------------------ council files
    private const STATUS_V = ['ok', 'failed', 'offline'];
    private const STATUS_J = ['full', 'thin', 'recovered', 'failed', 'offline', 'no_views'];
    private const TOP = ['schema', 'idea', 'adapter', 'synthetic', 'stakeholders', 'sources', 'rounds', 'decision_tree',
        'locked_decisions', 'flags'];
    private const VIEW_REC = ['stakeholder' => 's', 'role' => 's', 'status' => 's', 'w5h1' => 'w', 'done_so_far' => 's',
        'missing' => 'l', 'devils_advocate' => 's', 'contribution' => 's', 'sources' => 'l', 'error' => 's'];
    private const JUROR_REC = ['synthesis_status' => 's', 'merge_summary' => 's', 'decision' => 's', 'resolved' => 'l',
        'open_issues' => 'l', 'next_steps' => 'l', 'reversals' => 'l', 'spec_delta' => 's', 'loop_recommended' => 'b',
        'error' => 's'];
    private const ROUND_REC = ['step_id' => 's', 'step_title' => 's', 'iteration' => 'i', 'question' => 's', 'views' => 'L',
        'juror' => 'J'];
    private const NODE_REC = ['step_id' => 's', 'iteration' => 'i', 'branches' => 'l', 'decision' => 's', 'next_steps' => 'l'];

    private static function goodStr(mixed $x): bool { return is_string($x) && !Py::hasLone($x); }

    private static function strList(mixed $v): bool
    {
        if (!Py::isList($v)) return false;
        foreach ($v as $x) if (!self::goodStr($x)) return false;
        return true;
    }

    private static function typeOk(string $t, mixed $v): bool
    {
        return match ($t) {
            's' => self::goodStr($v),
            'i' => (is_int($v) && $v >= 1) || ($v instanceof BigInt && $v->digits[0] !== '-'),
            'b' => is_bool($v),
            'l' => self::strList($v),
            'w' => $v instanceof Obj && self::sameKeys($v, self::W5H1_KEYS) && self::strList($v->values()),
            'L' => Py::isList($v),
            'J' => $v instanceof Obj,
        };
    }

    private static function typed(array &$errs, string $where, mixed $d, array $spec): void
    {
        $keys = array_keys($spec);
        if (!$d instanceof Obj || !self::sameKeys($d, $keys)) {
            $errs[] = "$where must have exactly the keys " . implode(', ', $keys);
            return;
        }
        foreach ($spec as $k => $t) if (!self::typeOk($t, $d->get($k))) $errs[] = "$where.$k has the wrong type";
    }

    /** A council.json written by an engine of this package: exact keys and JSON types everywhere, known
     *  statuses, no unpaired surrogates. At most 50 errors. */
    public static function checkCouncil(mixed $d): array
    {
        if (!$d instanceof Obj) return ['a council file must be a JSON object'];
        if ($d->get('schema') !== 'smartsavant.council/1') return ['schema must be "smartsavant.council/1"'];
        if (!self::sameKeys($d, self::TOP)) return ['top-level keys must be exactly ' . implode(', ', Py::sorted(self::TOP))];
        $errs = [];
        foreach (['idea', 'adapter'] as $k) if (!self::goodStr($d->get($k))) $errs[] = "$k must be a string";
        if (!is_bool($d->get('synthetic'))) $errs[] = 'synthetic must be true or false';
        foreach (['stakeholders', 'sources', 'locked_decisions'] as $k) {
            if (!self::strList($d->get($k))) $errs[] = "$k must be a list of strings";
        }
        $rounds = $d->get('rounds');
        $tree = $d->get('decision_tree');
        $flags = $d->get('flags');
        if (!Py::isList($rounds) || !Py::isList($tree) || !Py::isList($flags)) {
            return [...$errs, 'rounds, decision_tree and flags must be lists'];
        }
        if (count($tree) !== count($rounds)) $errs[] = 'decision_tree must have one node per round';
        foreach ($rounds as $i => $r) {
            $w = "rounds[$i]";
            self::typed($errs, $w, $r, self::ROUND_REC);
            if (!$r instanceof Obj || !Py::isList($r->get('views')) || !$r->get('juror') instanceof Obj) continue;
            foreach ($r->get('views') as $j => $v) {
                self::typed($errs, "$w.views[$j]", $v, self::VIEW_REC);
                if ($v instanceof Obj && !in_array($v->get('status'), self::STATUS_V, true)) {
                    $errs[] = "$w.views[$j].status must be one of " . implode(', ', self::STATUS_V);
                }
            }
            self::typed($errs, "$w.juror", $r->get('juror'), self::JUROR_REC);
            if (!in_array($r->get('juror')->get('synthesis_status'), self::STATUS_J, true)) {
                $errs[] = "$w.juror.synthesis_status must be one of " . implode(', ', self::STATUS_J);
            }
        }
        foreach ($tree as $i => $n) self::typed($errs, "decision_tree[$i]", $n, self::NODE_REC);
        foreach ($flags as $i => $f) {
            $ok = $f instanceof Obj;
            if ($ok) foreach ($f as $x) if (!(self::goodStr($x) || Py::isInt($x))) $ok = false;
            if (!$ok) $errs[] = "flags[$i] must be an object of strings and integers";
        }
        return array_slice($errs, 0, 50);
    }
}
