package cloud.smarttasks.smartsavant;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;

/**
 * Audit a council run (or its stories, or its trainset) before anyone builds or trains from it. The verdict is
 * "build-ready" only when no finding has severity "error". Codes (stable):
 * <pre>
 *   AUDIT-NO-SYNTHESIS          error    a phase's last merge is not a full synthesis
 *   AUDIT-OFFLINE-VIEWS         error    views from an offline template (warning when fewer than half)
 *   AUDIT-FAILED-VIEWS          warning  views the adapter never delivered validly
 *   AUDIT-FALLBACK-TEXT         error    fallback text in summaries, decisions or spec deltas
 *   AUDIT-TEMPLATE-VIEWS        error    the same view text repeated across 3 or more phases
 *   AUDIT-GENERIC-STEPS         warning  next steps or story titles that name nothing (error when all are)
 *   AUDIT-PLACEHOLDER-CODE      warning  "TODO" / return {'ok': True} code presented as content
 *   AUDIT-UNSOURCED             warning  figures without a source, or sources the run did not provide
 *   AUDIT-COMPLIANCE            error    a compliance rule fired
 *   AUDIT-SYNTHETIC             error    a synthetic run (stub, demo)
 *   AUDIT-TRAINSET-CONTAMINATED error    trainset rows that teach fallback or placeholder output
 *   AUDIT-TRAINSET-UNREADABLE   error    trainset rows that are not objects with an output or messages
 * </pre>
 * A council is not taken at its word: a merge recorded as full that fails synthesisOk counts as not decided,
 * and the adapters stub, scripted and demo* count as synthetic whatever the file says.
 */
public final class Audit {
    private Audit() {}

    public static final int MAX_EXAMPLES = 5;
    private static final List<String> SYNTHETIC_ADAPTERS = List.of("stub", "scripted");

    private static Map<String, Object> finding(String code, String severity, long count, String detail, List<?> examples) {
        List<Object> ex = new ArrayList<>();
        for (Object e : examples.subList(0, Math.min(MAX_EXAMPLES, examples.size()))) ex.add(Py.head(Py.str(e), 200));
        return Py.obj("code", code, "severity", severity, "count", count, "detail", detail, "examples", ex);
    }

    private static Map<String, Object> juror(Object r) { return Py.map(Py.map(r).get("juror")); }

    private static boolean decided(Object r) {
        return "full".equals(juror(r).get("synthesis_status")) && Gates.synthesisOk(juror(r));
    }

    public static Map<String, Object> auditResult(Map<String, Object> res) {
        Map<String, Object> h = Council.health(res);
        List<Map<String, Object>> out = new ArrayList<>();
        List<Map<String, Object>> last = Exporters.lastRounds(res);
        List<Object> rounds = Py.l(res, "rounds");
        List<String> undecided = new ArrayList<>();
        for (Map<String, Object> r : last) {
            if (decided(r)) continue;
            String st = Py.str(juror(r).get("synthesis_status"));
            undecided.add(Py.str(r.get("step_id")) + ": " + st + ("full".equals(st) ? " (recorded, but it fails the synthesis gate)" : ""));
        }
        if (!undecided.isEmpty()) {
            out.add(finding("AUDIT-NO-SYNTHESIS", "error", undecided.size(),
                undecided.size() + " of " + last.size() + " phases have no full synthesis", undecided));
        }
        int nViews = 0;
        List<String> off = new ArrayList<>();
        List<String> failed = new ArrayList<>();
        for (Object r : rounds) {
            for (Object v : Py.l(r, "views")) {
                nViews++;
                String where = Py.str(Py.get(r, "step_id")) + "/" + Py.str(Py.get(v, "stakeholder"));
                if ("offline".equals(Py.get(v, "status"))) off.add(where);
                if ("failed".equals(Py.get(v, "status"))) failed.add(where + ": " + Py.str(Py.get(v, "error")));
            }
        }
        if (!off.isEmpty()) {
            String sev = 2 * off.size() >= Math.max(1, nViews) ? "error" : "warning";
            out.add(finding("AUDIT-OFFLINE-VIEWS", sev, off.size(), off.size() + " of " + nViews + " views are offline templates", off));
        }
        if (!failed.isEmpty()) out.add(finding("AUDIT-FAILED-VIEWS", "warning", failed.size(), failed.size() + " views were never valid", failed));
        List<String> fb = new ArrayList<>();
        for (Object r : rounds) {
            Map<String, Object> j = juror(r);
            List<Object[]> fields = new ArrayList<>();
            fields.add(new Object[] {"merge_summary", j.get("merge_summary")});
            fields.add(new Object[] {"decision", j.get("decision")});
            fields.add(new Object[] {"spec_delta", j.get("spec_delta")});
            for (String k : List.of("resolved", "next_steps", "open_issues")) {
                List<Object> items = Py.l(j, k);
                for (int i = 0; i < items.size(); i++) fields.add(new Object[] {k + "[" + i + "]", items.get(i)});
            }
            for (Object[] f : fields) {
                if (Gates.isFallback(f[1])) fb.add(Py.str(Py.get(r, "step_id")) + "." + f[0] + ": " + Py.head(Py.str(f[1]), 80));
            }
        }
        if (!fb.isEmpty()) out.add(finding("AUDIT-FALLBACK-TEXT", "error", fb.size(), "fallback text where a decision should be", fb));
        // the same devil's-advocate or contribution text in several phases: a template
        Map<List<Object>, Set<String>> byText = new LinkedHashMap<>();
        for (Object r : rounds) {
            String step = Py.str(Py.get(r, "step_id"));
            String base = step.indexOf('.') >= 0 ? step.substring(0, step.indexOf('.')) : step;
            for (Object v : Py.l(r, "views")) {
                for (String f : List.of("devils_advocate", "contribution")) {
                    String sq = Gates.squash(Py.str(Py.get(v, f)));
                    if (Py.len(sq) >= 20) byText.computeIfAbsent(Arrays.asList(Py.get(v, "stakeholder"), sq), k -> new LinkedHashSet<>()).add(base);
                }
            }
        }
        List<String> tmpl = new ArrayList<>();
        for (Map.Entry<List<Object>, Set<String>> e : byText.entrySet()) {
            if (e.getValue().size() >= 3) tmpl.add(Py.str(e.getKey().get(0)) + ": " + Py.head((String) e.getKey().get(1), 60));
        }
        if (!tmpl.isEmpty()) out.add(finding("AUDIT-TEMPLATE-VIEWS", "error", tmpl.size(), "the same view text appears in 3 or more phases", tmpl));
        List<Object> steps = new ArrayList<>();
        for (Map<String, Object> r : last) steps.addAll(Py.l(juror(r), "next_steps"));
        List<Object> gen = new ArrayList<>();
        for (Object s : steps) if (Gates.isGeneric(s)) gen.add(s);
        if (!gen.isEmpty()) {
            String sev = gen.size() == steps.size() ? "error" : "warning";
            out.add(finding("AUDIT-GENERIC-STEPS", sev, gen.size(), gen.size() + " of " + steps.size() + " next steps name nothing", gen));
        }
        List<String> ph = new ArrayList<>();
        for (Object r : rounds) {
            for (Object v : Py.l(r, "views")) {
                if (Gates.hasPlaceholderCode(Py.get(v, "contribution"))) ph.add(Py.str(Py.get(r, "step_id")) + "/" + Py.str(Py.get(v, "stakeholder")));
            }
        }
        for (Object r : rounds) if (Gates.hasPlaceholderCode(juror(r).get("spec_delta"))) ph.add(Py.str(Py.get(r, "step_id")) + ".spec_delta");
        if (!ph.isEmpty()) out.add(finding("AUDIT-PLACEHOLDER-CODE", "warning", ph.size(), "placeholder code presented as content", ph));
        List<Object> flags = Py.l(res, "flags");
        if (!flags.isEmpty()) {
            List<String> ex = new ArrayList<>();
            for (Object x : flags) {
                Object fig = Py.get(x, "figure");
                ex.add(Py.str(Py.get(x, "step_id")) + "/" + Py.str(Py.get(x, "stakeholder")) + ": "
                    + Py.str(Py.truthy(fig) ? fig : Py.get(x, "source")));
            }
            out.add(finding("AUDIT-UNSOURCED", "warning", flags.size(), "figures without a source, or unknown sources", ex));
        }
        List<String> text = new ArrayList<>();
        text.add(Py.str(res.get("idea")));
        for (Object r : rounds) {
            Map<String, Object> j = juror(r);
            text.add(Py.str(j.get("merge_summary")));
            text.add(Py.str(j.get("decision")));
            text.add(Py.str(j.get("spec_delta")));
            for (Object v : Py.l(r, "views")) text.add(Py.str(Py.get(v, "contribution")));
        }
        List<String> comp = Gates.complianceBlock(String.join(" ", text));
        if (!comp.isEmpty()) out.add(finding("AUDIT-COMPLIANCE", "error", comp.size(), "compliance rules fired", comp));
        String adapter = Py.str(res.get("adapter"));
        if (Boolean.TRUE.equals(res.get("synthetic")) || SYNTHETIC_ADAPTERS.contains(adapter) || adapter.startsWith("demo")) {
            out.add(finding("AUDIT-SYNTHETIC", "error", 1, "adapter " + adapter + " is synthetic: not a real deliberation", List.of()));
        }
        return report("council", h, out);
    }

    public static Map<String, Object> auditStories(Object doc) {
        Object st = doc instanceof Map<?, ?> ? Py.get(doc, "stories", List.of()) : List.of();
        List<Object> stories = st instanceof List<?> ? Py.list(st) : List.of();
        List<Object> titles = new ArrayList<>();
        List<Object> blocked = new ArrayList<>();
        for (Object s : stories) {
            if (!(s instanceof Map<?, ?>)) continue;
            titles.add(Py.get(s, "title", ""));
            if ("blocked".equals(Py.get(s, "status"))) blocked.add(Py.get(s, "story_id", ""));
        }
        List<Object> gen = new ArrayList<>();
        for (Object t : titles) if (Gates.isGeneric(t)) gen.add(t);
        List<Map<String, Object>> out = new ArrayList<>();
        if (!gen.isEmpty()) {
            String sev = gen.size() == titles.size() ? "error" : "warning";
            out.add(finding("AUDIT-GENERIC-STEPS", sev, gen.size(), gen.size() + " of " + titles.size() + " story titles name nothing", gen));
        }
        if (!blocked.isEmpty()) {
            out.add(finding("AUDIT-NO-SYNTHESIS", "error", blocked.size(), blocked.size() + " stories are blocked (no decision)", blocked));
        }
        if (titles.isEmpty()) out.add(finding("AUDIT-NO-SYNTHESIS", "error", 0, "no stories", List.of()));
        return report("stories", Py.obj("stories", (long) titles.size()), out);
    }

    private static String rowText(Object row) {
        if (!(row instanceof Map<?, ?>)) return "";
        Object msgs = Py.get(row, "messages");
        if (msgs instanceof List<?> l) {
            List<String> parts = new ArrayList<>();
            for (Object m : l) if (m instanceof Map<?, ?> && "assistant".equals(Py.get(m, "role"))) parts.add(Py.str(Py.get(m, "content", "")));
            return String.join(" ", parts);
        }
        return Py.str(Py.get(row, "output", ""));
    }

    public static Map<String, Object> auditTrainset(List<?> rows) {
        List<String> bad = new ArrayList<>();
        List<String> unreadable = new ArrayList<>();
        Map<String, Long> kinds = new TreeMap<>(Py::cmp);
        for (int i = 0; i < rows.size(); i++) {
            Object row = rows.get(i);
            if (!(row instanceof Map<?, ?>) || !(Py.get(row, "output") instanceof String || Py.get(row, "messages") instanceof List<?>)) {
                unreadable.add("row " + i + ": not an object with an output or messages");
                continue;
            }
            Object meta = Py.get(row, "meta", Map.of());
            Object k = meta instanceof Map<?, ?> ? Py.get(meta, "kind") : null;
            String kind = k instanceof String s ? s : "unknown";
            kinds.merge(kind, 1L, Long::sum);
            String t = rowText(row);
            List<String> why = new ArrayList<>();
            if (Gates.isFallback(t)) why.add("fallback");
            if (Gates.hasPlaceholderCode(t)) why.add("placeholder code");
            if (!why.isEmpty()) bad.add("row " + i + " (" + kind + "): " + String.join(", ", why));
        }
        List<Map<String, Object>> out = new ArrayList<>();
        if (!unreadable.isEmpty()) {
            out.add(finding("AUDIT-TRAINSET-UNREADABLE", "error", unreadable.size(),
                unreadable.size() + " of " + rows.size() + " rows cannot be used", unreadable));
        }
        if (!bad.isEmpty()) {
            out.add(finding("AUDIT-TRAINSET-CONTAMINATED", "error", bad.size(),
                bad.size() + " of " + rows.size() + " rows teach fallback or placeholder output", bad));
        }
        if (rows.isEmpty()) out.add(finding("AUDIT-TRAINSET-CONTAMINATED", "error", 0, "no rows", List.of()));
        List<Object> kindList = new ArrayList<>();
        for (Map.Entry<String, Long> e : kinds.entrySet()) kindList.add(new ArrayList<Object>(List.of(e.getKey(), e.getValue())));
        return report("trainset", Py.obj("rows", (long) rows.size(), "kinds", kindList), out);
    }

    private static int order(Object severity) {
        return "error".equals(severity) ? 0 : "warning".equals(severity) ? 1 : 2;
    }

    private static Map<String, Object> report(String kind, Map<String, Object> summary, List<Map<String, Object>> findings) {
        findings.sort((a, b) -> {
            int c = Integer.compare(order(a.get("severity")), order(b.get("severity")));
            return c != 0 ? c : Py.cmp((String) a.get("code"), (String) b.get("code"));
        });
        long errors = findings.stream().filter(f -> "error".equals(f.get("severity"))).count();
        return Py.obj("kind", kind, "summary", summary, "findings", new ArrayList<Object>(findings),
            "verdict", errors == 0 ? "build-ready" : "not build-ready");
    }

    public static final int MAX_ROWS = 200_000;
    public static final int MAX_CHARS = 256 * 1024 * 1024;
    public static final int MAX_LINE = 1024 * 1024;
    public static final int MAX_LINES = 1_000_000;

    /** Parsed rows (null for a line that is not JSON), stopping at the row, size and line budgets. */
    public static List<Object> readJsonl(List<String> lines) {
        long rowsLeft = MAX_ROWS;
        long charsLeft = MAX_CHARS;
        long linesLeft = MAX_LINES;
        List<Object> rows = new ArrayList<>();
        for (String raw : lines) {
            charsLeft -= Py.len(raw);
            linesLeft -= 1;
            if (charsLeft < 0 || rowsLeft <= 0 || linesLeft < 0) break;
            String line = Py.strip(raw, " \t\r\n");
            if (line.isEmpty()) continue;
            rowsLeft -= 1;
            if (Py.len(line) > MAX_LINE) {
                rows.add(null);
                continue;
            }
            try {
                rows.add(Json.parse(line));
            } catch (IllegalArgumentException e) {
                rows.add(null);
            }
        }
        return rows;
    }

    /**
     * Audit one file: a council JSON (this package or SmartSignal), a stories JSON or a .jsonl trainset.
     * Zip archives are read by the Python reference only.
     */
    public static Map<String, Object> auditPath(Path file) throws IOException {
        String name = file.getFileName().toString();
        if (name.endsWith(".zip")) throw new IllegalArgumentException("zip trainsets are read by the Python reference only");
        String body = Files.readString(file, StandardCharsets.UTF_8);
        if (name.endsWith(".jsonl")) {
            List<String> lines = new ArrayList<>();
            String norm = body.replace("\r\n", "\n").replace("\r", "\n");
            int st = 0;
            for (int k = 0; k < norm.length(); k++) {
                if (norm.charAt(k) == '\n') {
                    lines.add(norm.substring(st, k + 1));
                    st = k + 1;
                }
            }
            if (st < norm.length()) lines.add(norm.substring(st));
            return auditTrainset(readJsonl(lines));
        }
        Object doc = Json.parse(body);
        String kind = Importer.detect(doc);
        if ("smartsavant".equals(kind)) {
            List<String> errs = Schema.checkCouncil(doc);
            if (!errs.isEmpty()) {
                throw new IllegalArgumentException("not a valid council file: " + String.join("; ", errs.subList(0, Math.min(3, errs.size()))));
            }
            return auditResult(Model.resultFromDict(Py.map(doc)));
        }
        if (kind != null) {
            Map<String, Object> rep = auditResult(Importer.fromSmartsignal(doc));
            rep.put("source", kind);
            return rep;
        }
        if (doc instanceof Map<?, ?> && Py.get(doc, "stories") instanceof List<?>) return auditStories(doc);
        throw new IllegalArgumentException("not a council result, a council export, a stories file or a trainset");
    }
}
