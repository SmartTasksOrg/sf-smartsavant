package cloud.smarttasks.smartsavant;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.function.Function;

/**
 * The council engine: roster, phases, stakeholder views, juror merge, decision tree.
 * <pre>
 *   Map&lt;String, Object&gt; res = new Council("...", new StubAdapter()).run();
 * </pre>
 * Every answer is copied to plain data, validated ({@link Schema#checkView} / {@link Schema#checkMerge}) and
 * retried; one that still fails is recorded as failed with an error, never filled with invented text.
 * Deterministic for a deterministic adapter. Not thread-safe; the adapter is called one call at a time.
 */
public final class Council {
    public static final List<String> DEFAULT_ROSTER = List.of("Developer", "UX/UI", "Product Owner", "Architecture/Infra");

    public static final List<List<String>> DEFAULT_PHASES = List.of(
        List.of("frame", "Frame", "What is the problem, who are the users, and the jobs-to-be-done?"),
        List.of("concept", "Concept", "What is the solution direction and the wedge?"),
        List.of("design", "Design", "What are the user flow, data flow, and experience?"),
        List.of("build", "Build", "What is the technical approach?"),
        List.of("integrate", "Integrate", "How does it fit the target architecture?"),
        List.of("measure", "Measure", "What are the KPIs and success metrics?"),
        List.of("spec", "Spec", "Assemble the final document."));

    private final String idea;
    private final LLMAdapter adapter;
    private final List<String> roster;
    private final List<List<String>> phases;
    private final List<Object> sources;
    private final int maxLoops;
    private final int retries;

    /** A council with the default roster and phases, no sources, max_loops 1 and retries 1. */
    public Council(String idea, LLMAdapter adapter) { this(idea, adapter, null, null, null, 1, 1); }

    /**
     * @param roster   stakeholder names (null or empty: {@link #DEFAULT_ROSTER})
     * @param phases   [step_id, title, question] lists or {step_id, title, question} maps (null or empty: the defaults)
     * @param sources  source ids views may cite (null: none)
     * @param maxLoops 1 to 10
     * @param retries  0 to 5
     */
    public Council(String idea, LLMAdapter adapter, List<?> roster, List<?> phases, List<?> sources, int maxLoops, int retries) {
        if (idea == null || Gates.blank(idea)) throw new IllegalArgumentException("the idea must be a non-empty string");
        List<Object> texts = new ArrayList<>();
        texts.add(idea);
        if (roster != null) texts.addAll(roster);
        if (sources != null) texts.addAll(sources);
        if (phases != null) for (Object p : phases) texts.addAll(phaseTriple(p));
        for (Object t : texts) {
            if (t instanceof String s && Py.hasLone(s)) {
                throw new IllegalArgumentException("the idea, roster, phases and sources must not contain unpaired surrogates");
            }
        }
        if (adapter == null) throw new IllegalArgumentException("adapter must be an LLMAdapter");
        List<String> r = new ArrayList<>();
        if (roster != null && !roster.isEmpty()) {
            for (Object x : roster) {
                if (!(x instanceof String s) || Gates.blank(s)) throw new IllegalArgumentException("roster names must be unique non-empty strings");
                r.add(s);
            }
            if (new HashSet<>(r).size() != r.size()) throw new IllegalArgumentException("roster names must be unique non-empty strings");
        } else {
            r.addAll(DEFAULT_ROSTER);
        }
        this.roster = r;
        this.phases = phases != null && !phases.isEmpty() ? checkPhases(phases) : DEFAULT_PHASES;
        if (maxLoops < 1 || maxLoops > 10) throw new IllegalArgumentException("max_loops is an integer from 1 to 10");
        if (retries < 0 || retries > 5) throw new IllegalArgumentException("retries is an integer from 0 to 5");
        this.idea = idea;
        this.adapter = adapter;
        this.sources = sources == null ? new ArrayList<>() : new ArrayList<>(sources);
        this.maxLoops = maxLoops;
        this.retries = retries;
    }

    /** A phase given as a list (or array) or as a {step_id, title, question} map, as a list of its parts. */
    private static List<Object> phaseTriple(Object p) {
        if (p instanceof Map<?, ?>) return Arrays.asList(Py.get(p, "step_id"), Py.get(p, "title"), Py.get(p, "question"));
        if (p instanceof List<?> l) return new ArrayList<>(l);
        if (p instanceof Object[] a) return Arrays.asList(a);
        return List.of();
    }

    /** [step_id, title, question] triples with unique step ids and non-blank strings, or IllegalArgumentException. */
    public static List<List<String>> checkPhases(List<?> phases) {
        List<List<String>> out = new ArrayList<>();
        Set<String> seen = new HashSet<>();
        for (Object p : phases) {
            boolean shape = p instanceof Map<?, ?> || p instanceof List<?> || p instanceof Object[];
            List<Object> t = phaseTriple(p);
            if (!shape || t.size() != 3 || !t.stream().allMatch(x -> x instanceof String s && !Gates.blank(s))) {
                throw new IllegalArgumentException("each phase is (step_id, title, question), three non-blank strings");
            }
            String id = (String) t.get(0);
            if (!seen.add(id)) throw new IllegalArgumentException("duplicate step_id " + id);
            out.add(List.of(id, (String) t.get(1), (String) t.get(2)));
        }
        if (out.isEmpty()) throw new IllegalArgumentException("at least one phase");
        return out;
    }

    /** Phases from a phases file in the format of spec/phases.json. */
    public static List<List<String>> loadPhases(Path file) throws IOException {
        Object doc = Json.parse(Files.readString(file, StandardCharsets.UTF_8));
        Object ph = doc instanceof Map<?, ?> ? Py.get(doc, "phases") : null;
        if (!(ph instanceof List<?> l) || l.isEmpty()) {
            throw new IllegalArgumentException("a phases file is {\"phases\": [{\"step_id\", \"title\", \"question\"}, ...]}");
        }
        for (Object p : l) if (!(p instanceof Map<?, ?>)) throw new IllegalArgumentException("each phase is an object {\"step_id\", \"title\", \"question\"}");
        return checkPhases(l);
    }

    private static final String NOT_PRINTABLE = "(message not printable)";

    /**
     * "Name: message" as the Python reference writes it: the exception's simple class name, its message
     * ("" when it has none; "(message not printable)" when reading it throws), lone surrogates as '?',
     * secrets redacted, then cut to 500 code points. Never throws.
     */
    public static String errorText(Throwable e) {
        String name;
        try {
            name = e.getClass().getSimpleName();
            if (name.isEmpty()) name = e.getClass().getName();
        } catch (RuntimeException x) {
            name = "Exception";
        }
        String msg;
        try {
            msg = e.getMessage();
            if (msg == null) msg = "";
        } catch (RuntimeException x) {
            msg = NOT_PRINTABLE;
        }
        msg = Py.replaceLone(msg, "?");
        return Py.head(Gates.redact(name + ": " + msg), 500);
    }

    @FunctionalInterface
    private interface Call {
        Object call() throws Exception;
    }

    /** {answer, ""} or {null, error} after 1 + retries attempts. */
    private Object[] ask(Call call, Function<Object, List<String>> check) {
        String err = "";
        for (int n = 0; n < 1 + retries; n++) {
            Object ans;
            try {
                ans = Schema.plain(call.call()); // validate exactly what will be used
            } catch (Schema.NotPlainData e) {
                err = Py.head("invalid: " + e.getMessage(), 500);
                continue;
            } catch (Exception e) { // an adapter failure is data, not a crash
                err = errorText(e);
                continue;
            }
            List<String> problems = check.apply(ans);
            if (problems.isEmpty()) return new Object[] {ans, ""};
            List<String> cut = new ArrayList<>();
            for (String p : problems.subList(0, Math.min(5, problems.size()))) cut.add(Py.head(p, 200));
            err = Py.head("invalid: " + String.join("; ", cut), 500);
        }
        return new Object[] {null, err};
    }

    /** Runs the council; returns the CouncilResult as an ordered map (plain JSON data). */
    public Map<String, Object> run() {
        String name;
        boolean synthetic;
        name = adapter.name() == null ? "adapter" : adapter.name();
        synthetic = adapter.synthetic();
        List<Object> locked = new ArrayList<>();
        List<Object> rounds = new ArrayList<>();
        List<Object> tree = new ArrayList<>();
        List<Object> flags = new ArrayList<>();
        Map<String, Object> res = Model.councilResult(Py.obj("idea", idea, "adapter", name, "synthetic", synthetic,
            "stakeholders", new ArrayList<Object>(roster), "sources", new ArrayList<>(sources), "rounds", rounds,
            "decision_tree", tree, "locked_decisions", locked, "flags", flags));
        List<Object> prior = new ArrayList<>();
        for (List<String> phase : phases) {
            String stepId = phase.get(0);
            String title = phase.get(1);
            String question = phase.get(2);
            long it = 0;
            while (true) {
                it += 1;
                Map<String, Object> ctx = Py.obj("idea", idea, "iteration", it, "sources", new ArrayList<>(sources),
                    "prior", prior, "locked", locked);
                List<Object> views = new ArrayList<>();
                List<Map<String, Object>> okViews = new ArrayList<>();
                for (String role : roster) {
                    Object[] got = ask(() -> adapter.view(role, role, stepId, question, Py.map(Py.clone(ctx))), Schema::checkView);
                    if (got[0] == null) {
                        views.add(Model.stakeholderView(Py.obj("stakeholder", role, "role", role, "status", "failed", "error", got[1])));
                        continue;
                    }
                    Map<String, Object> raw = Schema.normalizeView(Py.map(got[0]));
                    Map<String, Object> v = Model.stakeholderView(Py.obj("stakeholder", role, "role", role, "status", "ok",
                        "w5h1", raw.get("w5h1"), "done_so_far", raw.get("done_so_far"),
                        "missing", new ArrayList<>(Py.list(raw.get("missing"))), "devils_advocate", raw.get("devils_advocate"),
                        "contribution", raw.get("contribution"), "sources", new ArrayList<>(Py.list(raw.get("sources")))));
                    views.add(v);
                    okViews.add(v);
                    Map<String, Object> fv = new LinkedHashMap<>(raw);
                    fv.put("stakeholder", role);
                    for (Map<String, Object> fl : Gates.flagUnsourced(fv, sources)) {
                        fl.put("step_id", stepId);
                        fl.put("iteration", it);
                        flags.add(fl);
                    }
                }
                Map<String, Object> juror;
                if (okViews.isEmpty()) {
                    juror = Model.jurorMerge(Py.obj("synthesis_status", "no_views", "error", "no valid view to merge"));
                } else {
                    List<Object> viewsCopy = Py.list(Py.clone(okViews));
                    List<Map<String, Object>> arg = new ArrayList<>();
                    for (Object x : viewsCopy) arg.add(Py.map(x));
                    Object[] got = ask(() -> {
                        List<Map<String, Object>> fresh = new ArrayList<>();
                        for (Object x : Py.list(Py.clone(arg))) fresh.add(Py.map(x));
                        return adapter.merge(stepId, question, fresh, Py.map(Py.clone(ctx)));
                    }, Schema::checkMerge);
                    if (got[0] == null) {
                        juror = Model.jurorMerge(Py.obj("synthesis_status", "failed", "error", got[1]));
                    } else {
                        Map<String, Object> m = Py.map(got[0]);
                        juror = Model.jurorMerge(Py.obj("synthesis_status", "full", "merge_summary", m.get("merge_summary"),
                            "decision", m.get("decision"), "resolved", new ArrayList<>(Py.list(m.get("resolved"))),
                            "open_issues", new ArrayList<>(Py.list(m.get("open_issues"))),
                            "next_steps", new ArrayList<>(Py.list(m.get("next_steps"))),
                            "reversals", new ArrayList<>(m.containsKey("reversals") ? Py.list(m.get("reversals")) : List.of()),
                            "spec_delta", m.get("spec_delta"), "loop_recommended", m.get("loop_recommended")));
                        if (!Gates.synthesisOk(juror)) juror.put("synthesis_status", "thin");
                    }
                }
                rounds.add(Model.councilRound(stepId, title, it, question, views, juror));
                List<Object> branches = new ArrayList<>();
                for (Map<String, Object> v : okViews) branches.add(v.get("contribution"));
                tree.add(Model.decisionNode(stepId, it, branches, juror.get("decision"), new ArrayList<>(Py.l(juror, "next_steps"))));
                if ("full".equals(juror.get("synthesis_status"))) {
                    for (Object x : Py.l(juror, "resolved")) if (!locked.contains(x)) locked.add(x);
                    prior = new ArrayList<>(prior);
                    prior.add(Py.obj("step", stepId, "decision", juror.get("decision")));
                }
                if (!Boolean.TRUE.equals(juror.get("loop_recommended")) || it >= maxLoops) break;
            }
        }
        return res;
    }

    /**
     * Counts for a run: views by status, merges by synthesis status, flags, and whether every phase's last
     * iteration reached a full synthesis (decided).
     */
    public static Map<String, Object> health(Map<String, Object> res) {
        Map<String, Long> views = new TreeMap<>(Py::cmp);
        Map<String, Long> merges = new TreeMap<>(Py::cmp);
        Map<Object, String> last = new LinkedHashMap<>();
        for (Object r : Py.l(res, "rounds")) {
            for (Object v : Py.l(r, "views")) views.merge(Py.s(v, "status"), 1L, Long::sum);
            String st = Py.s(Py.map(r).get("juror"), "synthesis_status");
            merges.merge(st, 1L, Long::sum);
            last.put(Py.map(r).get("step_id"), st);
        }
        long decided = last.values().stream().filter("full"::equals).count();
        return Py.obj("views", new LinkedHashMap<String, Object>(views), "merges", new LinkedHashMap<String, Object>(merges),
            "phases", (long) last.size(), "phases_decided", decided,
            "decided", !last.isEmpty() && decided == last.size(), "flags", (long) Py.l(res, "flags").size(),
            "synthetic", res.get("synthetic"));
    }
}
