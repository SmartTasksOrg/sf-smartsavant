package cloud.smarttasks.smartsavant;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Exporters: a council result to user stories, a spec in Markdown, a fine-tune trainset and a Mermaid tree.
 * Every exporter reports what the run did not decide instead of covering it up.
 */
public final class Exporters {
    private Exporters() {}

    /** sha256 of the canonical JSON of the whole result: the run's identity in exports. */
    public static String runDigest(Map<String, Object> res) { return Canonical.sha256Hex(Canonical.encode(res)); }

    /** The last iteration of each phase, in phase order. */
    static List<Map<String, Object>> lastRounds(Map<String, Object> res) {
        Map<Object, Map<String, Object>> last = new LinkedHashMap<>();
        for (Object r : Py.l(res, "rounds")) last.put(Py.map(r).get("step_id"), Py.map(r));
        return new ArrayList<>(last.values());
    }

    private static Map<String, Object> juror(Map<String, Object> r) { return Py.map(r.get("juror")); }

    public static Map<String, Object> toStories(Map<String, Object> res, String projectKey) {
        List<Object> stories = new ArrayList<>();
        long ready = 0;
        int n = 0;
        for (Map<String, Object> r : lastRounds(res)) {
            Map<String, Object> j = juror(r);
            String sid = String.format("STORY-%s-%04d", projectKey, ++n);
            List<Object> steps = new ArrayList<>();
            for (Object s : Py.l(j, "next_steps")) if (!Gates.isGeneric(s)) steps.add(s);
            if ("full".equals(j.get("synthesis_status")) && !steps.isEmpty()) {
                List<Object> ac = new ArrayList<>();
                ac.add("Implements the decision: " + Py.str(j.get("decision")));
                for (Object x : Py.l(j, "resolved")) ac.add("Holds: " + Py.str(x));
                ac.add("A test exercises it end to end.");
                stories.add(Py.obj("story_id", sid, "status", "ready", "epic", r.get("step_title"), "title", steps.get(0),
                    "decision", j.get("decision"), "sub_tasks", steps, "acceptance_criteria", ac,
                    "open_issues", new ArrayList<>(Py.l(j, "open_issues")), "phase", r.get("step_id"), "iteration", r.get("iteration")));
                ready++;
            } else {
                List<Object> issues = new ArrayList<>(Py.l(j, "open_issues"));
                if (Py.truthy(j.get("error"))) issues.add(j.get("error"));
                stories.add(Py.obj("story_id", sid, "status", "blocked", "epic", r.get("step_title"),
                    "title", "Decide " + Py.str(r.get("step_title")) + ": the council did not reach a decision ("
                        + Py.str(j.get("synthesis_status")) + ")",
                    "decision", "", "sub_tasks", new ArrayList<>(), "acceptance_criteria", new ArrayList<>(),
                    "open_issues", issues, "phase", r.get("step_id"), "iteration", r.get("iteration")));
            }
        }
        return Py.obj("schema", "smartsavant.stories/1", "project_key", projectKey, "run", runDigest(res),
            "counts", Py.obj("stories", (long) stories.size(), "ready", ready, "blocked", stories.size() - ready),
            "stories", stories);
    }

    private static final String MD_META = "\\`*_{}[]()#+-.!|~";

    /** Control (U+0000-U+001F, U+007F-U+009F) and bidirectional-control characters, shown as U+FFFD. */
    static boolean bad(char c) {
        return c < 0x20 || (c >= 0x7f && c <= 0x9f) || (c >= 0x202a && c <= 0x202e) || (c >= 0x2066 && c <= 0x2069);
    }

    /**
     * Model text made inert in Markdown: one line (whitespace squashed), control and bidirectional-control
     * characters as U+FFFD, &amp; &lt; &gt; as HTML entities, and every Markdown metacharacter
     * \ ` * _ { } [ ] ( ) # + - . ! | ~ backslash-escaped.
     */
    public static String md(Object text) {
        StringBuilder b = new StringBuilder();
        for (char c : Gates.squash(Py.str(text)).toCharArray()) {
            if (bad(c)) b.append('\ufffd');
            else if (c == '&') b.append("&amp;");
            else if (c == '<') b.append("&lt;");
            else if (c == '>') b.append("&gt;");
            else if (MD_META.indexOf(c) >= 0) b.append('\\').append(c);
            else b.append(c);
        }
        return b.toString();
    }

    public static String toSpecMd(Map<String, Object> res) {
        Map<String, Object> h = Council.health(res);
        List<String> lines = new ArrayList<>(List.of("# Council spec", "", "*" + md(res.get("idea")) + "*", "",
            "Run " + runDigest(res).substring(0, 16) + " with adapter " + md(res.get("adapter"))
                + (Boolean.TRUE.equals(res.get("synthetic")) ? " (synthetic: not a real deliberation)" : "") + ". "
                + h.get("phases_decided") + " of " + h.get("phases") + " phases decided; views " + Py.repr(h.get("views"))
                + "; merges " + Py.repr(h.get("merges")) + "; " + h.get("flags") + " flag(s).", ""));
        if (!Boolean.TRUE.equals(h.get("decided"))) {
            lines.addAll(List.of("> **Not build-ready:** at least one phase has no full synthesis. Do not build from the "
                + "undecided phases; re-run them with a working adapter.", ""));
        }
        List<Object> locked = Py.l(res, "locked_decisions");
        if (!locked.isEmpty()) {
            lines.addAll(List.of("## Decisions", "", "| # | Decision |", "|---|---|"));
            for (int i = 0; i < locked.size(); i++) lines.add("| " + (i + 1) + " | " + md(locked.get(i)) + " |");
            lines.add("");
        }
        for (Map<String, Object> r : lastRounds(res)) {
            Map<String, Object> j = juror(r);
            lines.addAll(List.of("## " + md(r.get("step_title")), "", "**Question:** " + md(r.get("question")), ""));
            if ("full".equals(j.get("synthesis_status"))) {
                lines.addAll(List.of("**Decision:** " + md(j.get("decision")), "", md(j.get("merge_summary")), ""));
                if (!Py.l(j, "next_steps").isEmpty()) {
                    lines.addAll(List.of("**Next steps:**", ""));
                    for (Object s : Py.l(j, "next_steps")) lines.add("- " + md(s));
                    lines.add("");
                }
            } else {
                lines.addAll(List.of("**NOT DECIDED** (" + Py.str(j.get("synthesis_status")) + ")"
                    + (Py.truthy(j.get("error")) ? ": " + md(j.get("error")) : ""), ""));
            }
            if (!Py.l(j, "open_issues").isEmpty()) {
                lines.addAll(List.of("**Open issues:**", ""));
                for (Object s : Py.l(j, "open_issues")) lines.add("- " + md(s));
                lines.add("");
            }
            List<String> failed = new ArrayList<>();
            for (Object v : Py.l(r, "views")) if (!"ok".equals(Py.get(v, "status"))) failed.add(Py.str(Py.get(v, "stakeholder")));
            if (!failed.isEmpty()) lines.addAll(List.of("*No valid view from: " + md(String.join(", ", failed)) + ".*", ""));
        }
        List<Object> flags = Py.l(res, "flags");
        if (!flags.isEmpty()) {
            lines.addAll(List.of("## Flags", "", flags.size() + " finding(s) to review before use:", ""));
            for (Object f : flags.subList(0, Math.min(200, flags.size()))) {
                Object fig = Py.get(f, "figure");
                Object src = Py.get(f, "source");
                Object what = Py.truthy(fig) ? fig : Py.truthy(src) ? src : "";
                lines.add("- " + Py.str(Py.get(f, "flag")) + ": " + md(what) + " (" + md(Py.get(f, "stakeholder", "")) + ", "
                    + md(Py.get(f, "step_id", "")) + ")");
            }
            lines.add("");
        }
        return String.join("\n", lines);
    }

    public static final Map<String, String> METHODOLOGY = Map.of(
        "frame", "Establish the problem, the real users and their jobs-to-be-done, and agree on one long-term goal.",
        "concept", "Collect solution directions, then pick the wedge: the smallest offer the users would switch for.",
        "design", "Draw the user flow and the data flow end to end, and name every screen and every store.",
        "build", "Decide the technical approach and the riskiest part to prove first.",
        "integrate", "Fit the design into the target architecture: owners, interfaces, auth, deployment.",
        "measure", "Set the KPIs, how each one is measured, and the thresholds that would change the plan.",
        "spec", "Assemble the decisions into one document that an engineer can build from.");

    private static final List<String> VIEW_FIELDS = List.of("w5h1", "done_so_far", "missing", "devils_advocate", "contribution", "sources");

    /** Rows {instruction, input, output, meta}: no expert or facilitation rows for a synthetic run. */
    public static List<Map<String, Object>> toTrainset(Map<String, Object> res) { return toTrainset(res, false); }

    /**
     * Rows {instruction, input, output, meta}: methodology per phase used; expert rows (ok views without a flag)
     * and facilitation rows (full merges) unless the run is synthetic and allowSynthetic is false.
     */
    public static List<Map<String, Object>> toTrainset(Map<String, Object> res, boolean allowSynthetic) {
        String digest = runDigest(res);
        List<Map<String, Object>> rows = new ArrayList<>();
        Set<List<Object>> flagged = new HashSet<>();
        for (Object f : Py.l(res, "flags")) {
            flagged.add(Arrays.asList(Py.get(f, "step_id"), key(Py.get(f, "iteration")), Py.get(f, "stakeholder")));
        }
        boolean learn = allowSynthetic || !Boolean.TRUE.equals(res.get("synthetic"));
        Set<Object> seen = new HashSet<>();
        for (Object ro : Py.l(res, "rounds")) {
            Map<String, Object> r = Py.map(ro);
            Object stepId = r.get("step_id");
            String title = Py.str(r.get("step_title"));
            Map<String, Object> meta = Py.obj("phase", stepId, "iteration", r.get("iteration"), "run", digest, "adapter", res.get("adapter"));
            if (!seen.contains(stepId) && stepId instanceof String sid && METHODOLOGY.containsKey(sid) && Py.lower(title).startsWith(sid)) {
                seen.add(stepId);
                Map<String, Object> m = new LinkedHashMap<>(meta);
                m.put("kind", "methodology");
                rows.add(Py.obj("instruction", "We are at the " + title + " phase of a design council. How should I facilitate it?",
                    "input", "", "output", METHODOLOGY.get(sid), "meta", m));
            }
            if (!learn) continue;
            for (Object vo : Py.l(r, "views")) {
                Map<String, Object> v = Py.map(vo);
                if ("ok".equals(v.get("status")) && !flagged.contains(Arrays.asList(stepId, key(r.get("iteration")), v.get("stakeholder")))) {
                    Map<String, Object> out = Py.obj();
                    for (String k : VIEW_FIELDS) out.put(k, v.get(k));
                    Map<String, Object> m = new LinkedHashMap<>(meta);
                    m.put("kind", "expert");
                    m.put("stakeholder", v.get("stakeholder"));
                    rows.add(Py.obj("instruction", "As the " + Py.str(v.get("stakeholder")) + " on a design council, give your view on: "
                        + Py.str(r.get("question")), "input", res.get("idea"), "output", Py.jsonDumps(out, true), "meta", m));
                }
            }
            Map<String, Object> j = juror(r);
            if ("full".equals(j.get("synthesis_status"))) {
                List<Object> contributions = new ArrayList<>();
                for (Object v : Py.l(r, "views")) if ("ok".equals(Py.get(v, "status"))) contributions.add(Py.get(v, "contribution"));
                Map<String, Object> m = new LinkedHashMap<>(meta);
                m.put("kind", "facilitation");
                m.put("synthesis", "full");
                rows.add(Py.obj("instruction", "Merge the panel for the " + title + " phase as the juror.",
                    "input", Py.jsonDumps(contributions, false),
                    "output", Py.jsonDumps(Py.obj("decision", j.get("decision"), "merge_summary", j.get("merge_summary"),
                        "resolved", j.get("resolved"), "next_steps", j.get("next_steps")), true),
                    "meta", m));
            }
        }
        return rows;
    }

    private static Object key(Object it) { return Py.isInt(it) ? (Object) Py.bigInt(it) : it; }

    /** A Mermaid label: squashed, control characters as U+FFFD, at most 80 code points, quotes as apostrophes,
     *  &amp; &lt; &gt; as entities, quoted. */
    static String label(Object t) {
        StringBuilder b = new StringBuilder();
        for (char c : Gates.squash(Py.str(t)).toCharArray()) b.append(bad(c) ? '\ufffd' : c);
        String s = Py.head(b.toString(), 80);
        return "\"" + s.replace("&", "&amp;").replace("\"", "'").replace("<", "&lt;").replace(">", "&gt;") + "\"";
    }

    public static String toMermaid(Map<String, Object> res) {
        List<String> lines = new ArrayList<>(List.of("flowchart TD", "  start([Idea])"));
        String prev = "start";
        List<Object> tree = Py.l(res, "decision_tree");
        List<Object> rounds = Py.l(res, "rounds");
        for (int i = 0; i < tree.size(); i++) {
            String nid = "n" + i;
            Object n = tree.get(i);
            Map<String, Object> r = Py.map(rounds.get(i));
            Map<String, Object> j = juror(r);
            Object text = "full".equals(j.get("synthesis_status")) ? Py.get(n, "decision") : "NOT DECIDED: " + Py.str(j.get("synthesis_status"));
            lines.add("  " + nid + "{" + label(Py.str(r.get("step_title")) + " #" + Py.str(Py.get(n, "iteration"))) + "}");
            lines.add("  " + prev + " --> " + nid);
            lines.add("  " + nid + "d[" + label(text) + "]");
            lines.add("  " + nid + " --> " + nid + "d");
            prev = nid;
        }
        return String.join("\n", lines) + "\n";
    }
}
