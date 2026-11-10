package cloud.smarttasks.smartsavant;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;

/**
 * Reads council exports written by other tools (the SmartSignal council) into a council result, so that the
 * gates, the audit and the exporters work on them. Two shapes are recognised: a council result (rounds with
 * views that hold the fields) and a rounds export (views with a "parsed" object). A view whose source is
 * "offline" becomes status offline; a full merge that fails synthesisOk becomes thin; object contributions
 * are flattened to "key: value" lines; a repeated (step_id, iteration) pair gets ".2", ".3", ... appended.
 * Type-robust: values of the wrong type are skipped or read as empty.
 */
public final class Importer {
    private Importer() {}

    private static final Set<String> STATUS = Set.of("full", "recovered", "offline", "failed", "thin", "no_views");
    public static final int MAX_DEPTH = 20;

    /** Unpaired surrogates (which cannot be written as UTF-8) become U+FFFD. */
    static String clean(String t) { return Py.replaceLone(t, "\ufffd"); }

    /** A value as text: strings as is, lists joined with "; ", objects as "key: value" lines, deeper than 20 levels "[nested too deep]". */
    public static String text(Object v) { return text(v, 0); }

    private static String text(Object v, int depth) {
        if (depth > MAX_DEPTH) return "[nested too deep]";
        if (v == null) return "";
        if (v instanceof String s) return clean(s);
        if (v instanceof Map<?, ?> m) {
            List<String> parts = new ArrayList<>();
            for (Map.Entry<?, ?> e : m.entrySet()) parts.add(text(e.getKey(), depth + 1) + ": " + text(e.getValue(), depth + 1));
            return String.join("\n", parts);
        }
        if (v instanceof List<?> l) {
            List<String> parts = new ArrayList<>();
            for (Object x : l) parts.add(text(x, depth + 1));
            return String.join("; ", parts);
        }
        if (v instanceof Boolean b) return b ? "true" : "false";
        if (Py.isInt(v)) return v.toString();
        if (v instanceof Number n) {
            double d = n.doubleValue();
            return Double.isNaN(d) || Double.isInfinite(d) ? "float" : Py.num(d);
        }
        return v.getClass().getSimpleName();
    }

    private static Map<String, Object> dict(Object v) { return v instanceof Map<?, ?> ? Py.map(v) : Map.of(); }

    private static List<Object> list(Object v) { return v instanceof List<?> ? Py.list(v) : List.of(); }

    private static List<Object> strs(Object v) {
        List<Object> out = new ArrayList<>();
        if (v == null) return out;
        if (v instanceof String s) {
            if (!Gates.blank(s)) out.add(s);
            return out;
        }
        if (v instanceof List<?> l) {
            for (Object x : l) {
                String t = text(x);
                if (!Gates.blank(t)) out.add(t);
            }
            return out;
        }
        out.add(text(v));
        return out;
    }

    /** The idea as text: a string as is; {name, problem, solution, audience} as one line. */
    public static String ideaText(Object idea) {
        if (idea instanceof String s) return s;
        if (idea instanceof Map<?, ?>) {
            String name = text(Py.get(idea, "name", ""));
            String prob = text(Py.get(idea, "problem", ""));
            String sol = text(Py.get(idea, "solution", ""));
            String aud = text(Py.get(idea, "audience", ""));
            String s = Py.strip(name + ": " + prob, ": ");
            if (!sol.isEmpty()) s += " -> " + sol;
            if (!aud.isEmpty()) s += " (audience: " + aud + ")";
            return s;
        }
        return text(idea);
    }

    /** "smartsavant", "smartsignal-rounds", "smartsignal-result" or null. */
    public static String detect(Object doc) {
        if (!(doc instanceof Map<?, ?>)) return null;
        if ("smartsavant.council/1".equals(Py.get(doc, "schema"))) return "smartsavant";
        Object rounds = Py.get(doc, "rounds");
        if (rounds instanceof List<?> l && !l.isEmpty() && l.stream().anyMatch(r -> r instanceof Map<?, ?> && Py.has(r, "views"))) {
            for (Object r : l) {
                for (Object v : list(Py.get(dict(r), "views"))) {
                    if (v instanceof Map<?, ?>) return Py.has(v, "parsed") ? "smartsignal-rounds" : "smartsignal-result";
                }
            }
            return "smartsignal-result";
        }
        return null;
    }

    /** Python's {@code type(x) is int and 0 <= x < 1000}. */
    private static boolean smallInt(Object x) {
        return Py.isInt(x) && Py.bigInt(x).signum() >= 0 && Py.bigInt(x).compareTo(java.math.BigInteger.valueOf(1000)) < 0;
    }

    /** A SmartSignal council export as a council result; IllegalArgumentException when doc is not one. */
    public static Map<String, Object> fromSmartsignal(Object doc) {
        String kind = detect(doc);
        if (!"smartsignal-rounds".equals(kind) && !"smartsignal-result".equals(kind)) {
            throw new IllegalArgumentException("not a SmartSignal council export (no rounds with views)");
        }
        Set<String> models = new TreeSet<>(Py::cmp);
        List<Object> rounds = new ArrayList<>();
        List<Object> tree = new ArrayList<>();
        List<Object> flags = new ArrayList<>();
        List<Object> locked = new ArrayList<>();
        Map<String, Object> res = Model.councilResult(Py.obj("idea", clean(ideaText(Py.get(doc, "idea", ""))), "synthetic", false,
            "rounds", rounds, "decision_tree", tree, "flags", flags, "locked_decisions", locked));
        Map<List<Object>, Integer> seen = new HashMap<>();
        List<String> names = new ArrayList<>();
        for (Object ro : Py.list(Py.get(doc, "rounds"))) {
            if (!(ro instanceof Map<?, ?>)) continue;
            Map<String, Object> r = Py.map(ro);
            String step = text(Py.get(r, "step_id", "step"));
            if (step.isEmpty()) step = "step";
            Object iter = Py.get(r, "iteration");
            Object rnd = Py.get(r, "round");
            long it = smallInt(iter) ? ((Number) iter).longValue() + 1 : smallInt(rnd) ? ((Number) rnd).longValue() + 1 : 1;
            List<Object> key = Arrays.asList(step, it);
            int count = seen.merge(key, 1, Integer::sum);
            if (count > 1) step = step + "." + count;
            List<Object> views = new ArrayList<>();
            for (Object vo : list(Py.get(r, "views"))) {
                if (!(vo instanceof Map<?, ?>)) continue;
                Map<String, Object> v = Py.map(vo);
                Map<String, Object> p = dict("smartsignal-rounds".equals(kind) ? Py.get(v, "parsed", v) : v);
                String src = text(Py.get(v, "source", ""));
                if (Py.truthy(Py.get(v, "model"))) models.add(text(v.get("model")));
                Map<String, Object> w = dict(Py.get(p, "w5h1"));
                String who = text(Py.get(v, "stakeholder", ""));
                if (!names.contains(who)) names.add(who);
                Map<String, Object> w5 = new LinkedHashMap<>();
                for (String k : Schema.W5H1_KEYS) w5.put(k, text(Py.get(w, k, "")));
                List<Object> sources = new ArrayList<>();
                for (Object s : strs(Py.get(p, "sources"))) if (Py.len((String) s) <= 200) sources.add(s);
                views.add(Model.stakeholderView(Py.obj("stakeholder", who, "role", text(Py.get(v, "role", "")),
                    "status", "offline".equals(src) ? "offline" : "ok", "w5h1", w5, "done_so_far", text(Py.get(p, "done_so_far")),
                    "missing", strs(Py.get(p, "missing")), "devils_advocate", text(Py.get(p, "devils_advocate")),
                    "contribution", text(Py.get(p, "contribution")), "sources", sources)));
            }
            Map<String, Object> j = dict(Py.get(r, "juror"));
            if (Py.truthy(Py.get(j, "model"))) models.add(text(j.get("model")));
            String st = text(Py.get(j, "synthesis_status", ""));
            if (st.isEmpty()) st = "failed";
            if (!STATUS.contains(st)) st = "failed";
            List<Object> resolved = strs(Py.get(j, "resolved"));
            String decision = text(Py.get(j, "decision"));
            if (decision.isEmpty()) decision = resolved.isEmpty() ? "" : (String) resolved.get(0);
            Map<String, Object> juror = Model.jurorMerge(Py.obj("synthesis_status", st, "merge_summary", text(Py.get(j, "merge_summary")),
                "decision", decision, "resolved", resolved, "open_issues", strs(Py.get(j, "open_issues")),
                "next_steps", strs(Py.get(j, "next_steps")), "reversals", strs(Py.get(j, "reversals")),
                "spec_delta", text(Py.get(j, "spec_delta")), "loop_recommended", Boolean.TRUE.equals(Py.get(j, "loop_recommended"))));
            if ("full".equals(st) && !Gates.synthesisOk(juror)) {
                juror.put("synthesis_status", "thin");
                juror.put("error", "imported as full, but it fails the synthesis gate");
            }
            String title = text(Py.get(r, "step_title", step));
            rounds.add(Model.councilRound(step, title.isEmpty() ? step : title, it,
                text(Py.get(r, "question", Py.get(r, "step_title", ""))), views, juror));
            List<Object> branches = new ArrayList<>();
            for (Object x : views) if ("ok".equals(Py.get(x, "status"))) branches.add(Py.get(x, "contribution"));
            tree.add(Model.decisionNode(step, it, branches, juror.get("decision"), new ArrayList<>(Py.l(juror, "next_steps"))));
            for (Object xo : views) {
                Map<String, Object> x = Py.map(xo);
                if (!"ok".equals(x.get("status"))) continue;
                Map<String, Object> fv = Py.obj("stakeholder", x.get("stakeholder"), "contribution", x.get("contribution"),
                    "devils_advocate", x.get("devils_advocate"), "done_so_far", x.get("done_so_far"), "missing", x.get("missing"),
                    "sources", x.get("sources"));
                for (Map<String, Object> fl : Gates.flagUnsourced(fv, List.of())) {
                    fl.put("step_id", step);
                    fl.put("iteration", it);
                    flags.add(fl);
                }
            }
            if ("full".equals(juror.get("synthesis_status"))) {
                for (Object d : Py.l(juror, "resolved")) if (!locked.contains(d)) locked.add(d);
            }
        }
        Object decs = Py.get(doc, "decisions");
        boolean anyFull = rounds.stream().anyMatch(r -> "full".equals(Py.get(Py.get(r, "juror"), "synthesis_status")));
        if (anyFull && decs instanceof List<?> dl && !dl.isEmpty() && dl.stream().allMatch(d -> d instanceof Map<?, ?>)) {
            List<Object> out = new ArrayList<>();
            for (Object d : dl) {
                String x = text(Py.get(d, "topic", "")) + ": " + text(Py.get(d, "decision", ""));
                if (!Gates.isFallback(x)) out.add(x);
            }
            res.put("locked_decisions", out);
        }
        List<Object> sh = new ArrayList<>();
        for (Object s : list(Py.get(doc, "stakeholders"))) sh.add(s instanceof Map<?, ?> ? text(Py.get(s, "name", "")) : text(s));
        res.put("stakeholders", sh.isEmpty() ? new ArrayList<Object>(names) : sh);
        res.put("adapter", models.isEmpty() ? "smartsignal" : "smartsignal:" + String.join(",", models));
        return res;
    }
}
