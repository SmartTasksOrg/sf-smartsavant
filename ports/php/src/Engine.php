<?php
// The data model, the adapter interface with the stub, scripted and replay adapters, and the council engine:
// roster -> phases -> stakeholder views -> juror merge -> decision tree.
//   $res = (new Council('...', new StubAdapter()))->run();   // an Obj: the council.json document
// Every answer is validated (checkView / checkMerge) and retried; one that still fails is recorded as failed
// with an error, never filled with invented text. Deterministic for a deterministic adapter.
declare(strict_types=1);

namespace SmartTasks\SmartSavant;

/** The council records as Obj, with fields in the order of the Python dataclasses (model.py). */
final class Model
{
    public const VIEW_STATUSES = ['ok', 'failed', 'offline'];
    public const SYNTHESIS_STATUSES = ['full', 'thin', 'recovered', 'failed', 'offline', 'no_views'];

    public static function w5h1(?Obj $f = null): Obj
    {
        $o = new Obj();
        foreach (Schema::W5H1_KEYS as $k) $o->set($k, $f?->get($k, '') ?? '');
        return $o;
    }

    public static function view(array $f): Obj
    {
        return Obj::of([
            'stakeholder' => $f['stakeholder'], 'role' => $f['role'], 'status' => $f['status'] ?? 'ok',
            'w5h1' => self::w5h1($f['w5h1'] ?? null), 'done_so_far' => $f['done_so_far'] ?? '', 'missing' => $f['missing'] ?? [],
            'devils_advocate' => $f['devils_advocate'] ?? '', 'contribution' => $f['contribution'] ?? '',
            'sources' => $f['sources'] ?? [], 'error' => $f['error'] ?? '',
        ]);
    }

    public static function juror(array $f = []): Obj
    {
        return Obj::of([
            'synthesis_status' => $f['synthesis_status'] ?? 'full', 'merge_summary' => $f['merge_summary'] ?? '',
            'decision' => $f['decision'] ?? '', 'resolved' => $f['resolved'] ?? [], 'open_issues' => $f['open_issues'] ?? [],
            'next_steps' => $f['next_steps'] ?? [], 'reversals' => $f['reversals'] ?? [], 'spec_delta' => $f['spec_delta'] ?? '',
            'loop_recommended' => $f['loop_recommended'] ?? false, 'error' => $f['error'] ?? '',
        ]);
    }

    public static function round(array $f): Obj
    {
        return Obj::of([
            'step_id' => $f['step_id'], 'step_title' => $f['step_title'], 'iteration' => $f['iteration'],
            'question' => $f['question'], 'views' => $f['views'] ?? [], 'juror' => $f['juror'] ?? self::juror(),
        ]);
    }

    public static function node(array $f): Obj
    {
        return Obj::of([
            'step_id' => $f['step_id'], 'iteration' => $f['iteration'], 'branches' => $f['branches'] ?? [],
            'decision' => $f['decision'] ?? '', 'next_steps' => $f['next_steps'] ?? [],
        ]);
    }

    public static function result(array $f = []): Obj
    {
        return Obj::of([
            'schema' => 'smartsavant.council/1', 'idea' => $f['idea'] ?? '', 'adapter' => $f['adapter'] ?? '',
            'synthetic' => $f['synthetic'] ?? false, 'stakeholders' => $f['stakeholders'] ?? [], 'sources' => $f['sources'] ?? [],
            'rounds' => $f['rounds'] ?? [], 'decision_tree' => $f['decision_tree'] ?? [],
            'locked_decisions' => $f['locked_decisions'] ?? [], 'flags' => $f['flags'] ?? [],
        ]);
    }

    /** A council.json document (check it with Schema::checkCouncil first) as a fresh copy. */
    public static function resultFromDict(Obj $d): Obj { return Py::copy($d); }
}

/**
 * The one boundary between the engine and a model. An adapter answers two calls and nothing else:
 *   view($stakeholder, $role, $phaseId, $question, $context)  -> a ViewAnswer
 *   merge($phaseId, $question, $views, $context)               -> a MergeAnswer
 * Answers are Obj, \stdClass or PHP arrays (spec/schema/*.json, bindings/council.d.ts). $context is a fresh
 * copy for every call: {idea, iteration, sources, prior, locked}. $views is a list of the valid views of the
 * phase (Obj records). Set $synthetic = true for adapters whose output is not a real deliberation.
 */
abstract class LLMAdapter
{
    public string $name = 'adapter';
    public bool $synthetic = false;

    abstract public function view(string $stakeholder, string $role, string $phaseId, string $question, Obj $context): mixed;

    /** @param list<Obj> $views */
    abstract public function merge(string $phaseId, string $question, array $views, Obj $context): mixed;
}

/** A deterministic adapter: no model, no network. Its tag is "sv" + the first 8 hex digits of
 *  sha256(stakeholder|phase_id|idea); "why" is the first 120 code points of the idea. Synthetic. */
class StubAdapter extends LLMAdapter
{
    public string $name = 'stub';
    public bool $synthetic = true;

    private static function tag(string ...$parts): string { return substr(hash('sha256', implode('|', $parts)), 0, 8); }

    public function view(string $stakeholder, string $role, string $phaseId, string $question, Obj $context): mixed
    {
        $idea = Py::str($context->get('idea', ''));
        $t = self::tag($stakeholder, $phaseId, $idea);
        return Obj::of([
            'w5h1' => Obj::of(['who' => $stakeholder, 'what' => "advance $phaseId", 'where' => 'the target architecture',
                'when' => 'this iteration', 'why' => Py::head($idea, 120), 'how' => "deterministic stub sv$t"]),
            'done_so_far' => "$role framed $phaseId.",
            'missing' => ["$phaseId: target files and modules"],
            'devils_advocate' => "Risk at $phaseId: an unproven assumption (sv$t).",
            'contribution' => "$role proposes the $phaseId step sv$t.",
            'sources' => [],
        ]);
    }

    public function merge(string $phaseId, string $question, array $views, Obj $context): mixed
    {
        $resolved = [];
        foreach ($views as $i => $v) $resolved[] = "$phaseId " . ($i + 1) . ': ' . Py::str($v->get('contribution'));
        $issues = [];
        foreach ($views as $v) {
            foreach (Py::iter($v->get('missing', [])) as $m) if (!in_array($m, $issues, true)) $issues[] = $m;
        }
        return Obj::of([
            'merge_summary' => 'Stub juror merged ' . count($views) . " view(s) for $phaseId.",
            'decision' => "$phaseId: proceed with the stub plan",
            'resolved' => array_slice($resolved, 0, 5),
            'open_issues' => array_slice($issues, 0, 5),
            'next_steps' => ["Write the $phaseId module for the stub plan"],
            'reversals' => [],
            'spec_delta' => "[$phaseId] stub decision recorded.",
            'loop_recommended' => false,
        ]);
    }
}

class AdapterError extends \RuntimeException
{
}

/** Python's KeyError / TypeError, raised where the reference would raise them on a malformed script. */
class KeyError extends \RuntimeException
{
}

class TypeError extends \RuntimeException
{
}

/**
 * Answers from a script: {"views": {"frame|1|Developer": [ANSWER, ...]}, "merges": {"build|1": [...]}}.
 * One entry per call, the last one repeats; {"raise": MESSAGE} throws AdapterError(MESSAGE); keys not in
 * the script get stub answers. Synthetic.
 */
class ScriptedAdapter extends StubAdapter
{
    public string $name = 'scripted';
    public bool $synthetic = true;
    private Obj $views;
    private Obj $merges;
    private array $n = [];

    public function __construct(?Obj $script = null)
    {
        $s = ($script !== null && count($script)) ? $script : new Obj();
        $this->views = Py::copy($s->get('views', new Obj()));
        $this->merges = Py::copy($s->get('merges', new Obj()));
    }

    private function next(Obj $table, string $key): mixed
    {
        $seq = $table->get($key);
        if (!Py::truthy($seq)) return null;
        $i = $this->n['k' . $key] ?? 0;
        $this->n['k' . $key] = $i + 1;
        if (is_string($seq)) $seq = Py::chars($seq);
        if ($seq instanceof Obj) throw new KeyError((string) min($i, count($seq) - 1));
        if (!is_array($seq)) throw new TypeError("'" . get_debug_type($seq) . "' object is not subscriptable");
        $ans = $seq[min($i, count($seq) - 1)];
        if ($ans instanceof Obj && $ans->keys() === ['raise']) throw new AdapterError(Py::str($ans->get('raise')));
        return Py::copy($ans);
    }

    public function view(string $stakeholder, string $role, string $phaseId, string $question, Obj $context): mixed
    {
        $key = "$phaseId|" . Py::str($context->get('iteration', 1)) . "|$stakeholder";
        if ($this->views->has($key)) return $this->next($this->views, $key);
        return parent::view($stakeholder, $role, $phaseId, $question, $context);
    }

    public function merge(string $phaseId, string $question, array $views, Obj $context): mixed
    {
        $key = "$phaseId|" . Py::str($context->get('iteration', 1));
        if ($this->merges->has($key)) return $this->next($this->merges, $key);
        return parent::merge($phaseId, $question, $views, $context);
    }
}

class ReplayError extends \RuntimeException
{
}

/** Replays the answers of a recorded run (a council.json written by an engine of this package). Views and
 *  merges whose status was not ok / full / thin are replayed as failures (ReplayError). */
class ReplayAdapter extends LLMAdapter
{
    public string $name = 'replay';
    private array $views = [];
    private array $merges = [];

    /** @param Obj|string $recorded a council document or the path of a council.json file */
    public function __construct(Obj|string $recorded)
    {
        $doc = is_string($recorded) ? Json::decode((string) file_get_contents($recorded)) : $recorded;
        $this->synthetic = Py::truthy($doc->get('synthetic', false));
        foreach ($doc->get('rounds', []) as $r) {
            $key = Py::str($r->get('step_id')) . "\0" . Py::str($r->get('iteration'));
            foreach ($r->get('views', []) as $v) $this->views[$key . "\0" . Py::str($v->get('stakeholder'))] ??= $v;
            $this->merges[$key] = $r->get('juror', new Obj());
        }
    }

    public function view(string $stakeholder, string $role, string $phaseId, string $question, Obj $context): mixed
    {
        $v = $this->views[$phaseId . "\0" . Py::str($context->get('iteration', 1)) . "\0" . $stakeholder] ?? null;
        if ($v === null || $v->get('status') !== 'ok') throw new ReplayError("no recorded valid view for $stakeholder at $phaseId");
        $out = new Obj();
        foreach (['w5h1', 'done_so_far', 'missing', 'devils_advocate', 'contribution', 'sources'] as $k) $out->set($k, $v->get($k));
        return Py::copy($out);
    }

    public function merge(string $phaseId, string $question, array $views, Obj $context): mixed
    {
        $m = $this->merges[$phaseId . "\0" . Py::str($context->get('iteration', 1))] ?? null;
        if ($m === null || !in_array($m->get('synthesis_status'), ['full', 'thin'], true)) {
            throw new ReplayError("no recorded valid merge for $phaseId");
        }
        $out = new Obj();
        foreach (Schema::MERGE_KEYS as $k) $out->set($k, $m->get($k));
        return Py::copy($out);
    }
}

final class Council
{
    public const DEFAULT_ROSTER = ['Developer', 'UX/UI', 'Product Owner', 'Architecture/Infra'];
    public const DEFAULT_PHASES = [
        ['frame', 'Frame', 'What is the problem, who are the users, and the jobs-to-be-done?'],
        ['concept', 'Concept', 'What is the solution direction and the wedge?'],
        ['design', 'Design', 'What are the user flow, data flow, and experience?'],
        ['build', 'Build', 'What is the technical approach?'],
        ['integrate', 'Integrate', 'How does it fit the target architecture?'],
        ['measure', 'Measure', 'What are the KPIs and success metrics?'],
        ['spec', 'Spec', 'Assemble the final document.'],
    ];
    private const NOT_PRINTABLE = '(message not printable)';

    public readonly string $idea;
    public readonly array $roster;
    public readonly array $phases;
    public readonly array $sources;
    public readonly int $maxLoops;
    public readonly int $retries;

    /**
     * @param array|null $roster  stakeholder names (null or [] for the default roster)
     * @param array|null $phases  [[step_id, title, question], ...] or [Obj{step_id, title, question}, ...]
     * @param array|null $sources the source ids views may cite
     */
    public function __construct(mixed $idea, public readonly LLMAdapter $adapter, ?array $roster = null, ?array $phases = null,
                                ?array $sources = null, mixed $maxLoops = 1, mixed $retries = 1)
    {
        if (!is_string($idea) || Gates::blank($idea)) throw new \InvalidArgumentException('the idea must be a non-empty string');
        $texts = [$idea, ...($roster ?? []), ...($sources ?? [])];
        foreach ($phases ?? [] as $p) if (Py::isList($p)) $texts = [...$texts, ...$p];
        foreach ($texts as $t) {
            if (is_string($t) && Py::hasLone($t)) {
                throw new \InvalidArgumentException('the idea, roster, phases and sources must not contain unpaired surrogates');
            }
        }
        $r = $roster ? array_values($roster) : self::DEFAULT_ROSTER;
        $ok = count(array_unique(array_map(fn ($x) => is_string($x) ? 's' . $x : serialize($x), $r))) === count($r);
        foreach ($r as $x) if (!is_string($x) || Gates::blank($x)) $ok = false;
        if (!$ok) throw new \InvalidArgumentException('roster names must be unique non-empty strings');
        $this->roster = $r;
        $this->phases = $phases ? self::checkPhases($phases) : self::DEFAULT_PHASES;
        if (!is_int($maxLoops) || $maxLoops < 1 || $maxLoops > 10) throw new \InvalidArgumentException('max_loops is an integer from 1 to 10');
        if (!is_int($retries) || $retries < 0 || $retries > 5) throw new \InvalidArgumentException('retries is an integer from 0 to 5');
        $this->idea = $idea;
        $this->sources = array_values($sources ?? []);
        $this->maxLoops = $maxLoops;
        $this->retries = $retries;
    }

    /** [[step_id, title, question]] with unique step ids and non-blank strings; an Obj phase is converted. */
    public static function checkPhases(array $phases): array
    {
        $out = [];
        $seen = [];
        foreach ($phases as $p) {
            if ($p instanceof Obj) $p = [$p->get('step_id'), $p->get('title'), $p->get('question')];
            $good = Py::isList($p) && count($p) === 3;
            if ($good) foreach ($p as $x) if (!is_string($x) || Gates::blank($x)) $good = false;
            if (!$good) throw new \InvalidArgumentException('each phase is (step_id, title, question), three non-blank strings');
            if (isset($seen['k' . $p[0]])) throw new \InvalidArgumentException("duplicate step_id {$p[0]}");
            $seen['k' . $p[0]] = true;
            $out[] = $p;
        }
        if (!$out) throw new \InvalidArgumentException('at least one phase');
        return $out;
    }

    /** A phases document ({"phases": [{step_id, title, question}, ...]}, an Obj or a file path). */
    public static function loadPhases(Obj|string|null $source = null): array
    {
        if ($source === null) return self::DEFAULT_PHASES;
        $doc = is_string($source) ? Json::decode((string) file_get_contents($source)) : $source;
        $ph = $doc instanceof Obj ? $doc->get('phases') : null;
        if (!Py::isList($ph) || !$ph) throw new \InvalidArgumentException('a phases file is {"phases": [{"step_id", "title", "question"}, ...]}');
        foreach ($ph as $p) if (!$p instanceof Obj) throw new \InvalidArgumentException('each phase is an object {"step_id", "title", "question"}');
        return self::checkPhases($ph);
    }

    /** "<ExceptionName>: <message>": lone surrogates as '?', secrets redacted, at most 500 code points. */
    public static function errorText(\Throwable $e): string
    {
        $name = get_class($e);
        $name = substr($name, (int) strrpos('\\' . $name, '\\'));
        try {
            $msg = $e->getMessage();
        } catch (\Throwable) {
            $msg = self::NOT_PRINTABLE;
        }
        return Py::head(Gates::redact($name . ': ' . Py::scrub($msg)), 500);
    }

    /** [answer, ''] or [null, error] after 1 + retries attempts. */
    private function ask(callable $call, callable $check): array
    {
        $err = '';
        for ($n = 0; $n < 1 + $this->retries; $n++) {
            try {
                $ans = Schema::plain($call());     // validate exactly what will be used
            } catch (NotPlainData $e) {
                $err = Py::head('invalid: ' . $e->getMessage(), 500);
                continue;
            } catch (\Throwable $e) {              // an adapter failure is data, not a crash
                $err = self::errorText($e);
                continue;
            }
            $problems = $check($ans);
            if (!$problems) return [$ans, ''];
            $err = Py::head('invalid: ' . implode('; ', array_map(fn ($p) => Py::head($p, 200), array_slice($problems, 0, 5))), 500);
        }
        return [null, $err];
    }

    /** Runs the council; returns the council.json document as an Obj. */
    public function run(): Obj
    {
        $a = $this->adapter;
        $rounds = [];
        $tree = [];
        $locked = [];
        $flags = [];
        $prior = [];
        foreach ($this->phases as [$stepId, $title, $question]) {
            $it = 0;
            while (true) {
                $it++;
                $ctx = Obj::of(['idea' => $this->idea, 'iteration' => $it, 'sources' => $this->sources, 'prior' => $prior,
                    'locked' => $locked]);
                $views = [];
                foreach ($this->roster as $role) {
                    [$got, $err] = $this->ask(fn () => $a->view($role, $role, $stepId, $question, Py::copy($ctx)),
                        [Schema::class, 'checkView']);
                    if ($got === null) {
                        $views[] = Model::view(['stakeholder' => $role, 'role' => $role, 'status' => 'failed', 'error' => $err]);
                        continue;
                    }
                    $raw = Schema::normalizeView($got);
                    $views[] = Model::view([
                        'stakeholder' => $role, 'role' => $role, 'status' => 'ok', 'w5h1' => $raw->get('w5h1'),
                        'done_so_far' => $raw->get('done_so_far'), 'missing' => $raw->get('missing'),
                        'devils_advocate' => $raw->get('devils_advocate'), 'contribution' => $raw->get('contribution'),
                        'sources' => $raw->get('sources'),
                    ]);
                    $fv = Py::copy($raw)->set('stakeholder', $role);
                    foreach (Gates::flagUnsourced($fv, $this->sources) as $fl) {
                        $flags[] = $fl->set('step_id', $stepId)->set('iteration', $it);
                    }
                }
                $okViews = array_values(array_filter($views, fn ($v) => $v->get('status') === 'ok'));
                if (!$okViews) {
                    $juror = Model::juror(['synthesis_status' => 'no_views', 'error' => 'no valid view to merge']);
                } else {
                    [$m, $err] = $this->ask(fn () => $a->merge($stepId, $question, Py::copy($okViews), Py::copy($ctx)),
                        [Schema::class, 'checkMerge']);
                    if ($m === null) {
                        $juror = Model::juror(['synthesis_status' => 'failed', 'error' => $err]);
                    } else {
                        $juror = Model::juror([
                            'synthesis_status' => 'full', 'merge_summary' => $m->get('merge_summary'), 'decision' => $m->get('decision'),
                            'resolved' => $m->get('resolved'), 'open_issues' => $m->get('open_issues'),
                            'next_steps' => $m->get('next_steps'), 'reversals' => $m->get('reversals', []),
                            'spec_delta' => $m->get('spec_delta'), 'loop_recommended' => $m->get('loop_recommended'),
                        ]);
                        if (!Gates::synthesisOk($juror)) $juror->set('synthesis_status', 'thin');
                    }
                }
                $rounds[] = Model::round(['step_id' => $stepId, 'step_title' => $title, 'iteration' => $it, 'question' => $question,
                    'views' => $views, 'juror' => $juror]);
                $tree[] = Model::node(['step_id' => $stepId, 'iteration' => $it,
                    'branches' => array_map(fn ($v) => $v->get('contribution'), $okViews),
                    'decision' => $juror->get('decision'), 'next_steps' => $juror->get('next_steps')]);
                if ($juror->get('synthesis_status') === 'full') {
                    foreach ($juror->get('resolved') as $r) if (!in_array($r, $locked, true)) $locked[] = $r;
                    $prior[] = Obj::of(['step' => $stepId, 'decision' => $juror->get('decision')]);
                }
                if (!$juror->get('loop_recommended') || $it >= $this->maxLoops) break;
            }
        }
        return Model::result([
            'idea' => $this->idea, 'adapter' => $a->name, 'synthetic' => $a->synthetic, 'stakeholders' => $this->roster,
            'sources' => $this->sources, 'rounds' => $rounds, 'decision_tree' => $tree, 'locked_decisions' => $locked,
            'flags' => $flags,
        ]);
    }

    /** Counts for a run: views by status, merges by synthesis status, flags, and whether every phase's last
     *  iteration reached a full synthesis (decided). */
    public static function health(Obj $res): Obj
    {
        $views = [];
        $merges = [];
        $last = [];
        foreach ($res->get('rounds') as $r) {
            foreach ($r->get('views') as $v) $views[$v->get('status')] = ($views[$v->get('status')] ?? 0) + 1;
            $st = $r->get('juror')->get('synthesis_status');
            $merges[$st] = ($merges[$st] ?? 0) + 1;
            $last['k' . $r->get('step_id')] = $st;
        }
        $sortedObj = function (array $m): Obj {
            $o = new Obj();
            $keys = array_map('strval', array_keys($m));
            foreach (Py::sorted($keys) as $k) $o->set($k, $m[$k]);
            return $o;
        };
        $st = array_values($last);
        return Obj::of([
            'views' => $sortedObj($views), 'merges' => $sortedObj($merges), 'phases' => count($last),
            'phases_decided' => count(array_filter($st, fn ($s) => $s === 'full')),
            'decided' => $st && count(array_filter($st, fn ($s) => $s !== 'full')) === 0,
            'flags' => count($res->get('flags')), 'synthetic' => $res->get('synthetic'),
        ]);
    }
}
