<?php
// The importer (SmartSignal council exports -> a council result) and the audit of a council run, its
// stories or its trainset. Codes and severities as in src/sf_smartsavant/audit.py:
//   AUDIT-NO-SYNTHESIS error, AUDIT-OFFLINE-VIEWS error (warning when fewer than half), AUDIT-FAILED-VIEWS
//   warning, AUDIT-FALLBACK-TEXT error, AUDIT-TEMPLATE-VIEWS error, AUDIT-GENERIC-STEPS warning (error when
//   all are), AUDIT-PLACEHOLDER-CODE warning, AUDIT-UNSOURCED warning, AUDIT-COMPLIANCE error,
//   AUDIT-SYNTHETIC error, AUDIT-TRAINSET-CONTAMINATED error, AUDIT-TRAINSET-UNREADABLE error.
declare(strict_types=1);

namespace SmartTasks\SmartSavant;

final class Importer
{
    private const STATUS = ['full', 'recovered', 'offline', 'failed', 'thin', 'no_views'];
    private const MAX_DEPTH = 20;

    /** Unpaired surrogates (which cannot be written as UTF-8) become U+FFFD. */
    private static function clean(string $t): string { return Py::replaceLone($t, "\u{FFFD}"); }

    public static function text(mixed $v, int $depth = 0): string
    {
        if ($depth > self::MAX_DEPTH) return '[nested too deep]';
        if ($v === null) return '';
        if (is_string($v)) return self::clean($v);
        if ($v instanceof Obj) {
            $parts = [];
            foreach ($v as $k => $x) $parts[] = self::text($k, $depth + 1) . ': ' . self::text($x, $depth + 1);
            return implode("\n", $parts);
        }
        if (is_array($v)) return implode('; ', array_map(fn ($x) => self::text($x, $depth + 1), $v));
        if (is_bool($v)) return $v ? 'true' : 'false';
        if (Py::isInt($v)) return (string) $v;
        if (is_float($v)) return is_finite($v) ? Py::floatRepr($v) : 'float';
        return get_debug_type($v);
    }

    private static function dict(mixed $v): Obj { return $v instanceof Obj ? $v : new Obj(); }
    private static function list(mixed $v): array { return Py::isList($v) ? $v : []; }

    private static function strs(mixed $v): array
    {
        if ($v === null) return [];
        if (is_string($v)) return Gates::blank($v) ? [] : [$v];
        if (Py::isList($v)) {
            $out = [];
            foreach ($v as $x) if (!Gates::blank(self::text($x))) $out[] = self::text($x);
            return $out;
        }
        return [self::text($v)];
    }

    public static function ideaText(mixed $idea): string
    {
        if (is_string($idea)) return $idea;
        if ($idea instanceof Obj) {
            [$name, $prob, $sol, $aud] = array_map(fn ($k) => self::text($idea->get($k, '')), ['name', 'problem', 'solution', 'audience']);
            $s = trim("$name: $prob", ': ');
            if ($sol !== '') $s .= " -> $sol";
            if ($aud !== '') $s .= " (audience: $aud)";
            return $s;
        }
        return self::text($idea);
    }

    /** 'smartsavant', 'smartsignal-rounds', 'smartsignal-result' or null. */
    public static function detect(mixed $doc): ?string
    {
        if (!$doc instanceof Obj) return null;
        if ($doc->get('schema') === 'smartsavant.council/1') return 'smartsavant';
        $rounds = $doc->get('rounds');
        if (!Py::isList($rounds) || !$rounds) return null;
        $any = false;
        foreach ($rounds as $r) if ($r instanceof Obj && $r->has('views')) $any = true;
        if (!$any) return null;
        foreach ($rounds as $r) {
            foreach (self::list(self::dict($r)->get('views')) as $v) {
                if ($v instanceof Obj) return $v->has('parsed') ? 'smartsignal-rounds' : 'smartsignal-result';
            }
        }
        return 'smartsignal-result';
    }

    private static function smallInt(mixed $x): bool { return is_int($x) && $x >= 0 && $x < 1000; }

    /** A SmartSignal council export as a council result (an Obj like Council::run() returns). */
    public static function fromSmartsignal(mixed $doc): Obj
    {
        $kind = self::detect($doc);
        if ($kind !== 'smartsignal-rounds' && $kind !== 'smartsignal-result') {
            throw new \InvalidArgumentException('not a SmartSignal council export (no rounds with views)');
        }
        $models = [];
        $seen = [];
        $names = [];
        $rounds = [];
        $tree = [];
        $flags = [];
        $locked = [];
        foreach ($doc->get('rounds') as $r) {
            if (!$r instanceof Obj) continue;
            $step = self::text($r->get('step_id', 'step'));
            if ($step === '') $step = 'step';
            $iter = $r->get('iteration');
            $rnd = $r->get('round');
            $it = self::smallInt($iter) ? $iter + 1 : (self::smallInt($rnd) ? $rnd + 1 : 1);
            $key = serialize([$step, $it]);
            $seen[$key] = ($seen[$key] ?? 0) + 1;
            if ($seen[$key] > 1) $step = "$step." . $seen[$key];
            $views = [];
            foreach (self::list($r->get('views')) as $v) {
                if (!$v instanceof Obj) continue;
                $p = self::dict($kind === 'smartsignal-rounds' ? $v->get('parsed', $v) : $v);
                $src = self::text($v->get('source', ''));
                if (Py::truthy($v->get('model'))) $models[] = self::text($v->get('model'));
                $w = self::dict($p->get('w5h1'));
                $who = self::text($v->get('stakeholder', ''));
                if (!in_array($who, $names, true)) $names[] = $who;
                $w5 = new Obj();
                foreach (Schema::W5H1_KEYS as $k) $w5->set($k, self::text($w->get($k, '')));
                $views[] = Model::view([
                    'stakeholder' => $who, 'role' => self::text($v->get('role', '')), 'status' => $src === 'offline' ? 'offline' : 'ok',
                    'w5h1' => $w5, 'done_so_far' => self::text($p->get('done_so_far')), 'missing' => self::strs($p->get('missing')),
                    'devils_advocate' => self::text($p->get('devils_advocate')), 'contribution' => self::text($p->get('contribution')),
                    'sources' => array_values(array_filter(self::strs($p->get('sources')), fn ($s) => Py::len($s) <= 200)),
                ]);
            }
            $j = self::dict($r->get('juror'));
            if (Py::truthy($j->get('model'))) $models[] = self::text($j->get('model'));
            $st = self::text($j->get('synthesis_status', ''));
            if ($st === '') $st = 'failed';
            if (!in_array($st, self::STATUS, true)) $st = 'failed';
            $resolved = self::strs($j->get('resolved'));
            $decision = self::text($j->get('decision'));
            $juror = Model::juror([
                'synthesis_status' => $st, 'merge_summary' => self::text($j->get('merge_summary')),
                'decision' => $decision !== '' ? $decision : ($resolved ? $resolved[0] : ''),
                'resolved' => $resolved, 'open_issues' => self::strs($j->get('open_issues')),
                'next_steps' => self::strs($j->get('next_steps')), 'reversals' => self::strs($j->get('reversals')),
                'spec_delta' => self::text($j->get('spec_delta')), 'loop_recommended' => $j->get('loop_recommended') === true,
            ]);
            if ($st === 'full' && !Gates::synthesisOk($juror)) {
                $juror->set('synthesis_status', 'thin');
                $juror->set('error', 'imported as full, but it fails the synthesis gate');
            }
            $okViews = array_values(array_filter($views, fn ($x) => $x->get('status') === 'ok'));
            $rounds[] = Model::round([
                'step_id' => $step, 'step_title' => ($t = self::text($r->get('step_title', $step))) !== '' ? $t : $step,
                'iteration' => $it, 'question' => self::text($r->get('question', $r->get('step_title', ''))), 'views' => $views,
                'juror' => $juror,
            ]);
            $tree[] = Model::node(['step_id' => $step, 'iteration' => $it,
                'branches' => array_map(fn ($x) => $x->get('contribution'), $okViews),
                'decision' => $juror->get('decision'), 'next_steps' => $juror->get('next_steps')]);
            foreach ($okViews as $x) {
                $fv = Obj::of(['stakeholder' => $x->get('stakeholder'), 'contribution' => $x->get('contribution'),
                    'devils_advocate' => $x->get('devils_advocate'), 'done_so_far' => $x->get('done_so_far'),
                    'missing' => $x->get('missing'), 'sources' => $x->get('sources')]);
                foreach (Gates::flagUnsourced($fv, []) as $fl) $flags[] = $fl->set('step_id', $step)->set('iteration', $it);
            }
            if ($juror->get('synthesis_status') === 'full') {
                foreach ($juror->get('resolved') as $d) if (!in_array($d, $locked, true)) $locked[] = $d;
            }
        }
        $decs = $doc->get('decisions');
        $anyFull = false;
        foreach ($rounds as $r) if ($r->get('juror')->get('synthesis_status') === 'full') $anyFull = true;
        if ($anyFull && Py::isList($decs) && $decs && count(array_filter($decs, fn ($d) => !$d instanceof Obj)) === 0) {
            $locked = [];
            foreach ($decs as $d) {
                $x = self::text($d->get('topic', '')) . ': ' . self::text($d->get('decision', ''));
                if (!Gates::isFallback($x)) $locked[] = $x;
            }
        }
        $sh = array_map(fn ($s) => $s instanceof Obj ? self::text($s->get('name', '')) : self::text($s),
            self::list($doc->get('stakeholders')));
        $models = array_values(array_unique($models));
        return Model::result([
            'idea' => self::clean(self::ideaText($doc->get('idea', ''))),
            'adapter' => $models ? 'smartsignal:' . implode(',', Py::sorted($models)) : 'smartsignal',
            'synthetic' => false, 'stakeholders' => $sh ?: $names, 'sources' => [], 'rounds' => $rounds,
            'decision_tree' => $tree, 'locked_decisions' => $locked, 'flags' => $flags,
        ]);
    }
}

final class Audit
{
    public const MAX_EXAMPLES = 5;
    private const SYNTHETIC_ADAPTERS = ['stub', 'scripted'];

    private static function finding(string $code, string $severity, int $count, string $detail, array $examples = []): Obj
    {
        return Obj::of(['code' => $code, 'severity' => $severity, 'count' => $count, 'detail' => $detail,
            'examples' => array_map(fn ($e) => Py::head(Py::str($e), 200), array_slice(array_values($examples), 0, self::MAX_EXAMPLES))]);
    }

    private static function decided(Obj $r): bool
    {
        return $r->get('juror')->get('synthesis_status') === 'full' && Gates::synthesisOk($r->get('juror'));
    }

    public static function auditResult(Obj $res): Obj
    {
        $h = Council::health($res);
        $out = [];
        $rounds = $res->get('rounds');
        $last = Exporters::lastRounds($res);
        $undecided = [];
        foreach ($last as $r) {
            if (self::decided($r)) continue;
            $st = $r->get('juror')->get('synthesis_status');
            $undecided[] = $r->get('step_id') . ": $st" . ($st !== 'full' ? '' : ' (recorded, but it fails the synthesis gate)');
        }
        if ($undecided) {
            $out[] = self::finding('AUDIT-NO-SYNTHESIS', 'error', count($undecided),
                count($undecided) . ' of ' . count($last) . ' phases have no full synthesis', $undecided);
        }
        $nviews = 0;
        $off = [];
        $failed = [];
        foreach ($rounds as $r) {
            foreach ($r->get('views') as $v) {
                $nviews++;
                if ($v->get('status') === 'offline') $off[] = $r->get('step_id') . '/' . $v->get('stakeholder');
                if ($v->get('status') === 'failed') $failed[] = $r->get('step_id') . '/' . $v->get('stakeholder') . ': ' . $v->get('error');
            }
        }
        if ($off) {
            $sev = 2 * count($off) >= max(1, $nviews) ? 'error' : 'warning';
            $out[] = self::finding('AUDIT-OFFLINE-VIEWS', $sev, count($off), count($off) . " of $nviews views are offline templates", $off);
        }
        if ($failed) $out[] = self::finding('AUDIT-FAILED-VIEWS', 'warning', count($failed), count($failed) . ' views were never valid', $failed);
        $fb = [];
        foreach ($rounds as $r) {
            $j = $r->get('juror');
            $fields = [['merge_summary', $j->get('merge_summary')], ['decision', $j->get('decision')], ['spec_delta', $j->get('spec_delta')]];
            foreach (['resolved', 'next_steps', 'open_issues'] as $k) {
                foreach ($j->get($k) as $i => $t) $fields[] = ["{$k}[$i]", $t];
            }
            foreach ($fields as [$name, $t]) {
                if (Gates::isFallback($t)) $fb[] = $r->get('step_id') . ".$name: " . Py::head($t, 80);
            }
        }
        if ($fb) $out[] = self::finding('AUDIT-FALLBACK-TEXT', 'error', count($fb), 'fallback text where a decision should be', $fb);
        // the same devil's-advocate or contribution text in several phases: a template
        $byText = [];
        foreach ($rounds as $r) {
            foreach ($r->get('views') as $v) {
                foreach ([$v->get('devils_advocate'), $v->get('contribution')] as $t) {
                    $sq = Gates::squash($t);
                    if (Py::len($sq) < 20) continue;
                    $key = serialize([$v->get('stakeholder'), $sq]);
                    $byText[$key] ??= [$v->get('stakeholder'), $sq, []];
                    $byText[$key][2]['k' . explode('.', $r->get('step_id'))[0]] = true;
                }
            }
        }
        $tmpl = [];
        foreach ($byText as [$who, $sq, $steps]) if (count($steps) >= 3) $tmpl[] = "$who: " . Py::head($sq, 60);
        if ($tmpl) $out[] = self::finding('AUDIT-TEMPLATE-VIEWS', 'error', count($tmpl), 'the same view text appears in 3 or more phases', $tmpl);
        $steps = [];
        foreach ($last as $r) foreach ($r->get('juror')->get('next_steps') as $s) $steps[] = $s;
        $gen = array_values(array_filter($steps, fn ($s) => Gates::isGeneric($s)));
        if ($gen) {
            $sev = count($gen) === count($steps) ? 'error' : 'warning';
            $out[] = self::finding('AUDIT-GENERIC-STEPS', $sev, count($gen), count($gen) . ' of ' . count($steps) . ' next steps name nothing', $gen);
        }
        $ph = [];
        foreach ($rounds as $r) {
            foreach ($r->get('views') as $v) {
                if (Gates::hasPlaceholderCode($v->get('contribution'))) $ph[] = $r->get('step_id') . '/' . $v->get('stakeholder');
            }
        }
        foreach ($rounds as $r) if (Gates::hasPlaceholderCode($r->get('juror')->get('spec_delta'))) $ph[] = $r->get('step_id') . '.spec_delta';
        if ($ph) $out[] = self::finding('AUDIT-PLACEHOLDER-CODE', 'warning', count($ph), 'placeholder code presented as content', $ph);
        $flags = $res->get('flags');
        if ($flags) {
            $ex = [];
            foreach ($flags as $x) {
                $what = $x->get('figure');
                if (!Py::truthy($what)) $what = $x->get('source');
                $ex[] = Py::str($x->get('step_id')) . '/' . Py::str($x->get('stakeholder')) . ': ' . Py::str($what);
            }
            $out[] = self::finding('AUDIT-UNSOURCED', 'warning', count($flags), 'figures without a source, or unknown sources', $ex);
        }
        $text = [$res->get('idea')];
        foreach ($rounds as $r) {
            $j = $r->get('juror');
            array_push($text, $j->get('merge_summary'), $j->get('decision'), $j->get('spec_delta'));
            foreach ($r->get('views') as $v) $text[] = $v->get('contribution');
        }
        $comp = Gates::complianceBlock(implode(' ', $text));
        if ($comp) $out[] = self::finding('AUDIT-COMPLIANCE', 'error', count($comp), 'compliance rules fired', $comp);
        $adapter = $res->get('adapter');
        if ($res->get('synthetic') || in_array($adapter, self::SYNTHETIC_ADAPTERS, true) || str_starts_with($adapter, 'demo')) {
            $out[] = self::finding('AUDIT-SYNTHETIC', 'error', 1, "adapter $adapter is synthetic: not a real deliberation");
        }
        return self::report('council', $h, $out);
    }

    public static function auditStories(mixed $doc): Obj
    {
        $stories = $doc instanceof Obj ? Py::iter($doc->get('stories', [])) : [];
        $titles = [];
        $blocked = [];
        foreach ($stories as $s) {
            if (!$s instanceof Obj) continue;
            $titles[] = $s->get('title', '');
            if ($s->get('status') === 'blocked') $blocked[] = $s->get('story_id', '');
        }
        $gen = array_values(array_filter($titles, fn ($t) => Gates::isGeneric($t)));
        $out = [];
        if ($gen) {
            $sev = count($gen) === count($titles) ? 'error' : 'warning';
            $out[] = self::finding('AUDIT-GENERIC-STEPS', $sev, count($gen), count($gen) . ' of ' . count($titles) . ' story titles name nothing', $gen);
        }
        if ($blocked) {
            $out[] = self::finding('AUDIT-NO-SYNTHESIS', 'error', count($blocked), count($blocked) . ' stories are blocked (no decision)', $blocked);
        }
        if (!$titles) $out[] = self::finding('AUDIT-NO-SYNTHESIS', 'error', 0, 'no stories');
        return self::report('stories', Obj::of(['stories' => count($titles)]), $out);
    }

    private static function rowText(Obj $row): string
    {
        $msgs = $row->get('messages');
        if (Py::isList($msgs)) {
            $parts = [];
            foreach ($msgs as $m) if ($m instanceof Obj && $m->get('role') === 'assistant') $parts[] = Py::str($m->get('content', ''));
            return implode(' ', $parts);
        }
        return Py::str($row->get('output', ''));
    }

    /** @param list<mixed> $rows trainset rows (null for a line that was not JSON) */
    public static function auditTrainset(array $rows): Obj
    {
        $bad = [];
        $unreadable = [];
        $kinds = [];
        foreach (array_values($rows) as $i => $row) {
            if (!$row instanceof Obj || !(is_string($row->get('output')) || Py::isList($row->get('messages')))) {
                $unreadable[] = "row $i: not an object with an output or messages";
                continue;
            }
            $meta = $row->get('meta', new Obj());
            $k = $meta instanceof Obj ? $meta->get('kind') : null;
            $k = is_string($k) ? $k : 'unknown';
            $kinds['k' . $k] = ($kinds['k' . $k] ?? 0) + 1;
            $t = self::rowText($row);
            $why = [];
            if (Gates::isFallback($t)) $why[] = 'fallback';
            if (Gates::hasPlaceholderCode($t)) $why[] = 'placeholder code';
            if ($why) $bad[] = "row $i ($k): " . implode(', ', $why);
        }
        $n = count($rows);
        $out = [];
        if ($unreadable) {
            $out[] = self::finding('AUDIT-TRAINSET-UNREADABLE', 'error', count($unreadable), count($unreadable) . " of $n rows cannot be used", $unreadable);
        }
        if ($bad) {
            $out[] = self::finding('AUDIT-TRAINSET-CONTAMINATED', 'error', count($bad), count($bad) . " of $n rows teach fallback or placeholder output", $bad);
        }
        if (!$rows) $out[] = self::finding('AUDIT-TRAINSET-CONTAMINATED', 'error', 0, 'no rows');
        $pairs = [];
        foreach (Py::sorted(array_map(fn ($k) => substr((string) $k, 1), array_keys($kinds))) as $k) $pairs[] = [$k, $kinds['k' . $k]];
        return self::report('trainset', Obj::of(['rows' => $n, 'kinds' => $pairs]), $out);
    }

    private static function report(string $kind, Obj $summary, array $findings): Obj
    {
        $order = ['error' => 0, 'warning' => 1, 'info' => 2];
        usort($findings, fn ($a, $b) => ($order[$a->get('severity')] <=> $order[$b->get('severity')])
            ?: strcmp($a->get('code'), $b->get('code')));
        $errors = count(array_filter($findings, fn ($f) => $f->get('severity') === 'error'));
        return Obj::of(['kind' => $kind, 'summary' => $summary, 'findings' => $findings,
            'verdict' => $errors === 0 ? 'build-ready' : 'not build-ready']);
    }

    public const MAX_ROWS = 200000;
    public const MAX_CHARS = 256 * 1024 * 1024;
    public const MAX_LINE = 1024 * 1024;
    public const MAX_LINES = 1000000;

    /** Parsed rows (null for a line that is not JSON), stopping at the row, size and line budgets. */
    public static function readJsonl(iterable $lines): array
    {
        [$rowsLeft, $chars, $linesLeft] = [self::MAX_ROWS, self::MAX_CHARS, self::MAX_LINES];
        $rows = [];
        foreach ($lines as $line) {
            $chars -= Py::len($line);
            $linesLeft--;
            if ($chars < 0 || $rowsLeft <= 0 || $linesLeft < 0) break;
            $line = trim($line, " \t\r\n");
            if ($line === '') continue;
            $rowsLeft--;
            if (Py::len($line) > self::MAX_LINE) {
                $rows[] = null;
                continue;
            }
            try {
                $rows[] = Json::decode($line);
            } catch (\JsonException) {
                $rows[] = null;
            }
        }
        return $rows;
    }

    /** Audit one file: a council JSON (an engine of this package, or SmartSignal), a stories JSON or a .jsonl
     *  trainset. Zip archives are read by the Python reference only. */
    public static function auditPath(string $path): Obj
    {
        if (str_ends_with($path, '.zip')) throw new \InvalidArgumentException('zip trainsets are read by the Python reference only');
        $body = file_get_contents($path);
        if ($body === false) throw new \RuntimeException("cannot read $path");
        if (str_ends_with($path, '.jsonl')) {
            $body = preg_match('//u', $body) === 1 ? $body : mb_scrub($body, 'UTF-8');
            return self::auditTrainset(self::readJsonl(preg_split('/(?<=\n)/', str_replace(["\r\n", "\r"], "\n", $body), -1, PREG_SPLIT_NO_EMPTY)));
        }
        $doc = Json::decode($body);
        if (Py::isList($doc) && $doc && count(array_filter($doc, fn ($r) => !($r instanceof Obj && $r->has('views')))) === 0) {
            $doc = Obj::of(['rounds' => $doc]);
        }
        $kind = Importer::detect($doc);
        if ($kind === 'smartsavant') {
            $errs = Schema::checkCouncil($doc);
            if ($errs) throw new \InvalidArgumentException('not a valid council file: ' . implode('; ', array_slice($errs, 0, 3)));
            return self::auditResult(Model::resultFromDict($doc));
        }
        if ($kind !== null) return self::auditResult(Importer::fromSmartsignal($doc))->set('source', $kind);
        if ($doc instanceof Obj && Py::isList($doc->get('stories'))) return self::auditStories($doc);
        throw new \InvalidArgumentException('not a council result, a council export, a stories file or a trainset');
    }
}
