<?php
// php bin/conformance.php [DIR]: runs spec/conformance/*.json against the PHP port, check for check as
// src/sf_smartsavant/conformance.py does. Prints the failures and a last line "php: N/N vectors passed".
declare(strict_types=1);
require __DIR__ . '/../src/SmartSavant.php';

use SmartTasks\SmartSavant\{Audit, Canonical, Council, Exporters, Gates, Importer, Json, Obj, Py, Schema, ScriptedAdapter};

function main(array $argv): int
{
    $dir = $argv[1] ?? __DIR__ . '/../../../spec/conformance';
    $load = fn (string $n) => Json::decode((string) file_get_contents("$dir/$n"));
    $show = fn (mixed $v) => Py::head(Json::py($v, true), 300);
    $total = 0;
    $fails = [];
    $check = function (string $name, callable $got, mixed $want) use (&$total, &$fails, $show): void {
        $total++;
        try {
            $g = $got();
        } catch (\Throwable $e) {
            $fails[] = "$name: raised " . get_class($e) . ': ' . $e->getMessage();
            return;
        }
        if (!Py::same($g, $want)) $fails[] = "$name: got {$show($g)}, want {$show($want)}";
    };

    foreach ($load('canonical.json')->cases as $c) {
        $check('canonical/' . $c->name, function () use ($c) {
            $t = Canonical::dumps($c->input);
            return [$t, hash('sha256', $t)];
        }, [$c->canonical, $c->sha256]);
    }
    foreach ($load('figures.json')->cases as $i => $c) $check("figures/$i", fn () => Gates::figures($c->text), $c->figures);
    $g = $load('gates.json')->cases;
    foreach ($g->flag_unsourced as $c) $check('flag_unsourced/' . $c->name, fn () => Gates::flagUnsourced($c->view, $c->allowed), $c->flags);
    $fns = ['is_generic' => 'isGeneric', 'is_fallback' => 'isFallback', 'has_placeholder_code' => 'hasPlaceholderCode',
        'compliance_block' => 'complianceBlock', 'squash' => 'squash'];
    foreach ($fns as $fn => $m) {
        foreach ($g->get($fn) as $i => $c) $check("$fn/$i", fn () => [Gates::class, $m]($c->text), $c->result);
    }
    foreach ($g->synthesis_ok as $c) $check('synthesis_ok/' . $c->name, fn () => Gates::synthesisOk($c->merge), $c->result);
    $s = $load('schema.json')->cases;
    foreach ($s->view as $c) $check('schema/view/' . $c->name, fn () => Schema::checkView($c->input), $c->errors);
    foreach ($s->merge as $c) $check('schema/merge/' . $c->name, fn () => Schema::checkMerge($c->input), $c->errors);
    foreach ($load('runs.json')->cases as $c) {
        $ph = Py::truthy($c->phases)
            ? array_map(fn ($p) => [$p->step_id, $p->title, $p->question], $c->phases) : null;
        try {
            $res = (new Council($c->idea, new ScriptedAdapter($c->script), $c->roster, $ph, $c->sources, $c->max_loops,
                $c->retries))->run();
        } catch (\Throwable $e) {
            foreach ($c->expect as $field => $_) {
                $total++;
                $fails[] = "runs/{$c->name}/$field: run raised " . get_class($e) . ': ' . $e->getMessage();
            }
            continue;
        }
        $k = $c->project_key;
        $got = [
            'council' => fn () => $res, 'digest' => fn () => Exporters::runDigest($res), 'health' => fn () => Council::health($res),
            'stories' => fn () => Exporters::toStories($res, $k), 'spec_md' => fn () => Exporters::toSpecMd($res),
            'mermaid' => fn () => Exporters::toMermaid($res), 'trainset' => fn () => Exporters::toTrainset($res),
            'trainset_all' => fn () => Exporters::toTrainset($res, true), 'audit' => fn () => Audit::auditResult($res),
        ];
        foreach ($c->expect as $field => $want) {
            $check("runs/{$c->name}/$field", $got[$field] ?? fn () => throw new \RuntimeException("unknown field $field"), $want);
        }
    }
    $a = $load('audit.json')->cases;
    foreach ($a->import as $c) $check('import/' . $c->name, fn () => Importer::fromSmartsignal($c->input), $c->council);
    foreach ($a->council as $c) $check('audit/council/' . $c->name, fn () => Audit::auditResult(Importer::fromSmartsignal($c->input)), $c->report);
    foreach ($a->stories as $c) $check('audit/stories/' . $c->name, fn () => Audit::auditStories($c->input), $c->report);
    foreach ($a->trainset as $c) $check('audit/trainset/' . $c->name, fn () => Audit::auditTrainset($c->input), $c->report);

    foreach ($fails as $f) echo $f, "\n";
    $passed = $total - count($fails);
    echo $fails ? "php: $passed/$total vectors passed (" . count($fails) . " failed)\n" : "php: $total/$total vectors passed\n";
    return $fails ? 1 : 0;
}

try {
    exit(main($argv));
} catch (\Throwable $e) {
    fwrite(STDERR, (string) $e . "\n");
    echo "php: FAILED\n";
    exit(1);
}
