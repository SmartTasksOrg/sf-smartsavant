package cloud.smarttasks.smartsavant;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.function.Function;
import java.util.function.Supplier;

/**
 * Runs spec/conformance against this port, check for check as src/sf_smartsavant/conformance.py does.
 * Usage: {@code java -cp build/classes cloud.smarttasks.smartsavant.Conformance [vector dir]}.
 * Prints the failures and a last line "java: N/N vectors passed"; exits 1 on any failure.
 */
public final class Conformance {
    private int total;
    private final List<String> fails = new ArrayList<>();

    private static String show(Object v) {
        String s;
        try {
            s = Py.jsonDumps(v, true);
        } catch (RuntimeException e) {
            s = String.valueOf(v);
        }
        return Py.head(s, 300);
    }

    private void check(String name, Supplier<Object> got, Object want) {
        total++;
        Object g;
        try {
            g = got.get();
        } catch (RuntimeException e) {
            fails.add(name + ": threw " + Council.errorText(e));
            return;
        }
        if (!Json.same(g, want)) fails.add(name + ": got " + show(g) + ", want " + show(want));
    }

    private static Map<String, Object> load(Path dir, String name) throws IOException {
        return Py.map(Json.parse(Files.readString(dir.resolve(name), StandardCharsets.UTF_8)));
    }

    private static List<Object> cases(Map<String, Object> doc, String key) { return Py.l(doc.get("cases"), key); }

    private void run(Path dir) throws IOException {
        for (Object c : Py.l(load(dir, "canonical.json"), "cases")) {
            check("canonical/" + Py.s(c, "name"), () -> {
                String t = Canonical.dumps(Py.get(c, "input"));
                return List.of(t, Canonical.sha256Hex(t));
            }, List.of(Py.get(c, "canonical"), Py.get(c, "sha256")));
        }
        List<Object> fig = Py.l(load(dir, "figures.json"), "cases");
        for (int i = 0; i < fig.size(); i++) {
            Object c = fig.get(i);
            check("figures/" + i, () -> Gates.figures(Py.get(c, "text")), Py.get(c, "figures"));
        }
        Map<String, Object> g = load(dir, "gates.json");
        for (Object c : cases(g, "flag_unsourced")) {
            check("flag_unsourced/" + Py.s(c, "name"), () -> Gates.flagUnsourced(Py.get(c, "view"), Py.get(c, "allowed")), Py.get(c, "flags"));
        }
        Map<String, Function<Object, Object>> fns = new java.util.LinkedHashMap<>();
        fns.put("is_generic", Gates::isGeneric);
        fns.put("is_fallback", Gates::isFallback);
        fns.put("has_placeholder_code", Gates::hasPlaceholderCode);
        fns.put("compliance_block", t -> Gates.complianceBlock(t));
        fns.put("squash", t -> Gates.squash((String) t));
        for (Map.Entry<String, Function<Object, Object>> e : fns.entrySet()) {
            List<Object> cs = cases(g, e.getKey());
            for (int i = 0; i < cs.size(); i++) {
                Object c = cs.get(i);
                check(e.getKey() + "/" + i, () -> e.getValue().apply(Py.get(c, "text")), Py.get(c, "result"));
            }
        }
        for (Object c : cases(g, "synthesis_ok")) {
            check("synthesis_ok/" + Py.s(c, "name"), () -> Gates.synthesisOk(Py.get(c, "merge")), Py.get(c, "result"));
        }
        Map<String, Object> s = load(dir, "schema.json");
        for (Object c : cases(s, "view")) check("schema/view/" + Py.s(c, "name"), () -> Schema.checkView(Py.get(c, "input")), Py.get(c, "errors"));
        for (Object c : cases(s, "merge")) check("schema/merge/" + Py.s(c, "name"), () -> Schema.checkMerge(Py.get(c, "input")), Py.get(c, "errors"));
        for (Object c : Py.l(load(dir, "runs.json"), "cases")) {
            String name = Py.s(c, "name");
            Map<String, Object> res;
            try {
                Object roster = Py.get(c, "roster");
                Object phases = Py.get(c, "phases");
                Object sources = Py.get(c, "sources");
                res = new Council(Py.s(c, "idea"), new ScriptedAdapter(Py.get(c, "script")),
                    roster == null ? null : Py.list(roster), phases == null ? null : Py.list(phases),
                    sources == null ? null : Py.list(sources),
                    ((Number) Py.get(c, "max_loops")).intValue(), ((Number) Py.get(c, "retries")).intValue()).run();
            } catch (RuntimeException e) {
                for (String f : Py.map(Py.get(c, "expect")).keySet()) {
                    total++;
                    fails.add("runs/" + name + "/" + f + ": the run threw " + Council.errorText(e));
                }
                continue;
            }
            String k = Py.s(c, "project_key");
            Map<String, Supplier<Object>> got = Map.of(
                "council", () -> res, "digest", () -> Exporters.runDigest(res), "health", () -> Council.health(res),
                "stories", () -> Exporters.toStories(res, k), "spec_md", () -> Exporters.toSpecMd(res),
                "mermaid", () -> Exporters.toMermaid(res), "trainset", () -> Exporters.toTrainset(res),
                "trainset_all", () -> Exporters.toTrainset(res, true), "audit", () -> Audit.auditResult(res));
            for (Map.Entry<String, Object> e : Py.map(Py.get(c, "expect")).entrySet()) {
                Supplier<Object> f = got.get(e.getKey());
                if (f == null) {
                    total++;
                    fails.add("runs/" + name + "/" + e.getKey() + ": unknown field");
                    continue;
                }
                check("runs/" + name + "/" + e.getKey(), f, e.getValue());
            }
        }
        Map<String, Object> a = load(dir, "audit.json");
        for (Object c : cases(a, "import")) {
            check("import/" + Py.s(c, "name"), () -> Importer.fromSmartsignal(Py.get(c, "input")), Py.get(c, "council"));
        }
        for (Object c : cases(a, "council")) {
            check("audit/council/" + Py.s(c, "name"), () -> Audit.auditResult(Importer.fromSmartsignal(Py.get(c, "input"))), Py.get(c, "report"));
        }
        for (Object c : cases(a, "stories")) {
            check("audit/stories/" + Py.s(c, "name"), () -> Audit.auditStories(Py.get(c, "input")), Py.get(c, "report"));
        }
        for (Object c : cases(a, "trainset")) {
            check("audit/trainset/" + Py.s(c, "name"), () -> Audit.auditTrainset(Py.list(Py.get(c, "input"))), Py.get(c, "report"));
        }
    }

    public static void main(String[] args) throws IOException {
        Path dir = Path.of(args.length > 0 ? args[0] : "../../spec/conformance");
        Conformance c = new Conformance();
        c.run(dir);
        for (String f : c.fails) System.out.println(f);
        int passed = c.total - c.fails.size();
        System.out.println(c.fails.isEmpty() ? "java: " + c.total + "/" + c.total + " vectors passed"
            : "java: " + passed + "/" + c.total + " vectors passed (" + c.fails.size() + " failed)");
        System.exit(c.fails.isEmpty() ? 0 : 1);
    }
}
