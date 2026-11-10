<?php
// Exporters: a council result -> user stories, a spec in Markdown, a fine-tune trainset, a Mermaid tree.
// Every exporter reports what the run did not decide instead of covering it up.
declare(strict_types=1);

namespace SmartTasks\SmartSavant;

final class Exporters
{
    public const METHODOLOGY = [
        'frame' => 'Establish the problem, the real users and their jobs-to-be-done, and agree on one long-term goal.',
        'concept' => 'Collect solution directions, then pick the wedge: the smallest offer the users would switch for.',
        'design' => 'Draw the user flow and the data flow end to end, and name every screen and every store.',
        'build' => 'Decide the technical approach and the riskiest part to prove first.',
        'integrate' => 'Fit the design into the target architecture: owners, interfaces, auth, deployment.',
        'measure' => 'Set the KPIs, how each one is measured, and the thresholds that would change the plan.',
        'spec' => 'Assemble the decisions into one document that an engineer can build from.',
    ];
    private const VIEW_FIELDS = ['w5h1', 'done_so_far', 'missing', 'devils_advocate', 'contribution', 'sources'];
    // control and bidirectional-control characters (C0, DEL, C1, U+202A-U+202E, U+2066-U+2069)
    private const BAD = '/[\x00-\x1F\x7F]|\xC2[\x80-\x9F]|\xE2\x80[\xAA-\xAE]|\xE2\x81[\xA6-\xA9]/';

    /** sha256 of the canonical JSON of the whole result: the run's identity in exports. */
    public static function runDigest(Obj $res): string { return hash('sha256', Canonical::dumps($res)); }

    /** The last iteration of each phase, in phase order. @return list<Obj> */
    public static function lastRounds(Obj $res): array
    {
        $last = [];
        foreach ($res->get('rounds') as $r) $last['k' . $r->get('step_id')] = $r;
        return array_values($last);
    }

    public static function toStories(Obj $res, string $projectKey): Obj
    {
        $stories = [];
        foreach (self::lastRounds($res) as $n => $r) {
            $j = $r->get('juror');
            $sid = sprintf('STORY-%s-%04d', $projectKey, $n + 1);
            $steps = array_values(array_filter($j->get('next_steps'), fn ($s) => !Gates::isGeneric($s)));
            if ($j->get('synthesis_status') === 'full' && $steps) {
                $stories[] = Obj::of([
                    'story_id' => $sid, 'status' => 'ready', 'epic' => $r->get('step_title'), 'title' => $steps[0],
                    'decision' => $j->get('decision'), 'sub_tasks' => $steps,
                    'acceptance_criteria' => ['Implements the decision: ' . $j->get('decision'),
                        ...array_map(fn ($x) => "Holds: $x", $j->get('resolved')), 'A test exercises it end to end.'],
                    'open_issues' => $j->get('open_issues'), 'phase' => $r->get('step_id'), 'iteration' => $r->get('iteration'),
                ]);
            } else {
                $stories[] = Obj::of([
                    'story_id' => $sid, 'status' => 'blocked', 'epic' => $r->get('step_title'),
                    'title' => 'Decide ' . $r->get('step_title') . ': the council did not reach a decision ('
                        . $j->get('synthesis_status') . ')',
                    'decision' => '', 'sub_tasks' => [], 'acceptance_criteria' => [],
                    'open_issues' => [...$j->get('open_issues'), ...($j->get('error') !== '' ? [$j->get('error')] : [])],
                    'phase' => $r->get('step_id'), 'iteration' => $r->get('iteration'),
                ]);
            }
        }
        $ready = count(array_filter($stories, fn ($s) => $s->get('status') === 'ready'));
        return Obj::of([
            'schema' => 'smartsavant.stories/1', 'project_key' => $projectKey, 'run' => self::runDigest($res),
            'counts' => Obj::of(['stories' => count($stories), 'ready' => $ready, 'blocked' => count($stories) - $ready]),
            'stories' => $stories,
        ]);
    }

    /** Model text made inert in Markdown: squashed to one line, control and bidirectional-control characters
     *  as U+FFFD, & < > as entities, and each of \ ` * _ { } [ ] ( ) # + - . ! | ~ backslash-escaped. */
    public static function md(mixed $text): string
    {
        $s = preg_replace(self::BAD, "\u{FFFD}", Gates::squash(Py::str($text)));
        $s = strtr($s, ['&' => '&amp;', '<' => '&lt;', '>' => '&gt;']);
        return preg_replace('/[\\\\`*_{}\[\]()#+\-.!|~]/', '\\\\$0', $s);
    }

    public static function toSpecMd(Obj $res): string
    {
        $h = Council::health($res);
        $lines = ['# Council spec', '', '*' . self::md($res->get('idea')) . '*', '',
            'Run ' . substr(self::runDigest($res), 0, 16) . ' with adapter ' . self::md($res->get('adapter'))
            . ($res->get('synthetic') ? ' (synthetic: not a real deliberation)' : '') . '. '
            . $h->get('phases_decided') . ' of ' . $h->get('phases') . ' phases decided; views ' . Py::repr($h->get('views'))
            . '; merges ' . Py::repr($h->get('merges')) . '; ' . $h->get('flags') . ' flag(s).', ''];
        if (!$h->get('decided')) {
            $lines[] = '> **Not build-ready:** at least one phase has no full synthesis. Do not build from the '
                . 'undecided phases; re-run them with a working adapter.';
            $lines[] = '';
        }
        $locked = $res->get('locked_decisions');
        if ($locked) {
            array_push($lines, '## Decisions', '', '| # | Decision |', '|---|---|');
            foreach ($locked as $i => $d) $lines[] = '| ' . ($i + 1) . ' | ' . self::md($d) . ' |';
            $lines[] = '';
        }
        foreach (self::lastRounds($res) as $r) {
            $j = $r->get('juror');
            array_push($lines, '## ' . self::md($r->get('step_title')), '', '**Question:** ' . self::md($r->get('question')), '');
            if ($j->get('synthesis_status') === 'full') {
                array_push($lines, '**Decision:** ' . self::md($j->get('decision')), '', self::md($j->get('merge_summary')), '');
                if ($j->get('next_steps')) {
                    array_push($lines, '**Next steps:**', '');
                    foreach ($j->get('next_steps') as $s) $lines[] = '- ' . self::md($s);
                    $lines[] = '';
                }
            } else {
                $lines[] = '**NOT DECIDED** (' . $j->get('synthesis_status') . ')'
                    . ($j->get('error') !== '' ? ': ' . self::md($j->get('error')) : '');
                $lines[] = '';
            }
            if ($j->get('open_issues')) {
                array_push($lines, '**Open issues:**', '');
                foreach ($j->get('open_issues') as $s) $lines[] = '- ' . self::md($s);
                $lines[] = '';
            }
            $failed = [];
            foreach ($r->get('views') as $v) if ($v->get('status') !== 'ok') $failed[] = $v->get('stakeholder');
            if ($failed) array_push($lines, '*No valid view from: ' . self::md(implode(', ', $failed)) . '.*', '');
        }
        $flags = $res->get('flags');
        if ($flags) {
            array_push($lines, '## Flags', '', count($flags) . ' finding(s) to review before use:', '');
            foreach (array_slice($flags, 0, 200) as $f) {
                $what = $f->get('figure');
                if (!Py::truthy($what)) $what = $f->get('source');
                if (!Py::truthy($what)) $what = '';
                $lines[] = '- ' . Py::str($f->get('flag')) . ': ' . self::md($what) . ' (' . self::md($f->get('stakeholder', ''))
                    . ', ' . self::md($f->get('step_id', '')) . ')';
            }
            $lines[] = '';
        }
        return implode("\n", $lines);
    }

    /** Rows {instruction, input, output, meta}: methodology per default phase used; expert rows (ok views
     *  without a flag) and facilitation rows (full merges) unless the run is synthetic. @return list<Obj> */
    public static function toTrainset(Obj $res, bool $allowSynthetic = false): array
    {
        $digest = self::runDigest($res);
        $rows = [];
        $flagged = [];
        foreach ($res->get('flags') as $f) {
            $flagged[serialize([$f->get('step_id'), $f->get('iteration'), $f->get('stakeholder')])] = true;
        }
        $learn = $allowSynthetic || !$res->get('synthetic');
        $seen = [];
        foreach ($res->get('rounds') as $r) {
            $sid = $r->get('step_id');
            $title = $r->get('step_title');
            $meta = fn (array $more) => Obj::of(['phase' => $sid, 'iteration' => $r->get('iteration'), 'run' => $digest,
                'adapter' => $res->get('adapter'), ...$more]);
            if (!isset($seen['k' . $sid]) && array_key_exists($sid, self::METHODOLOGY) && str_starts_with(Py::lower($title), $sid)) {
                $seen['k' . $sid] = true;
                $rows[] = Obj::of([
                    'instruction' => "We are at the $title phase of a design council. How should I facilitate it?",
                    'input' => '', 'output' => self::METHODOLOGY[$sid], 'meta' => $meta(['kind' => 'methodology']),
                ]);
            }
            if (!$learn) continue;
            foreach ($r->get('views') as $v) {
                $who = $v->get('stakeholder');
                if ($v->get('status') === 'ok' && !isset($flagged[serialize([$sid, $r->get('iteration'), $who])])) {
                    $out = new Obj();
                    foreach (self::VIEW_FIELDS as $k) $out->set($k, $v->get($k));
                    $rows[] = Obj::of([
                        'instruction' => "As the $who on a design council, give your view on: " . $r->get('question'),
                        'input' => $res->get('idea'), 'output' => Json::py($out, true),
                        'meta' => $meta(['kind' => 'expert', 'stakeholder' => $who]),
                    ]);
                }
            }
            $j = $r->get('juror');
            if ($j->get('synthesis_status') === 'full') {
                $ok = [];
                foreach ($r->get('views') as $v) if ($v->get('status') === 'ok') $ok[] = $v->get('contribution');
                $rows[] = Obj::of([
                    'instruction' => "Merge the panel for the $title phase as the juror.",
                    'input' => Json::py($ok),
                    'output' => Json::py(Obj::of(['decision' => $j->get('decision'), 'merge_summary' => $j->get('merge_summary'),
                        'resolved' => $j->get('resolved'), 'next_steps' => $j->get('next_steps')]), true),
                    'meta' => $meta(['kind' => 'facilitation', 'synthesis' => 'full']),
                ]);
            }
        }
        return $rows;
    }

    /** A Mermaid label: squashed, control characters as U+FFFD, at most 80 code points, & " < > replaced, quoted. */
    public static function label(mixed $t): string
    {
        $s = Py::head(preg_replace(self::BAD, "\u{FFFD}", Gates::squash(Py::str($t))), 80);
        return '"' . strtr($s, ['&' => '&amp;', '"' => "'", '<' => '&lt;', '>' => '&gt;']) . '"';
    }

    public static function toMermaid(Obj $res): string
    {
        $lines = ['flowchart TD', '  start([Idea])'];
        $prev = 'start';
        $rounds = $res->get('rounds');
        foreach ($res->get('decision_tree') as $i => $n) {
            $nid = "n$i";
            $r = $rounds[$i];
            $st = $r->get('juror')->get('synthesis_status');
            $label = $st === 'full' ? $n->get('decision') : "NOT DECIDED: $st";
            $lines[] = '  ' . $nid . '{' . self::label($r->get('step_title') . ' #' . Py::str($n->get('iteration'))) . '}';
            $lines[] = "  $prev --> $nid";
            $lines[] = "  {$nid}d[" . self::label($label) . ']';
            $lines[] = "  $nid --> {$nid}d";
            $prev = $nid;
        }
        return implode("\n", $lines) . "\n";
    }
}
