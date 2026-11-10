package cloud.smarttasks.smartsavant;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The council data model as ordered maps with the fields in the order of the Python dataclasses
 * (src/sf_smartsavant/model.py and bindings/council.d.ts). A result is plain JSON data: the canonical
 * writer hashes it and {@link Py#jsonDumps} writes a council.json the Python reference reads.
 * <pre>
 *   view.status              ok | failed | offline
 *   juror.synthesis_status   full | thin | recovered | failed | offline | no_views
 * </pre>
 */
public final class Model {
    private Model() {}

    public static final List<String> VIEW_STATUSES = List.of("ok", "failed", "offline");
    public static final List<String> SYNTHESIS_STATUSES = List.of("full", "thin", "recovered", "failed", "offline", "no_views");

    private static Object or(Map<String, Object> f, String k, Object d) {
        Object v = f.get(k);
        return v != null ? v : d;
    }

    /** {who, what, where, when, why, how}, "" for each field not given. */
    public static Map<String, Object> w5h1(Map<String, Object> f) {
        Map<String, Object> out = new LinkedHashMap<>();
        for (String k : Schema.W5H1_KEYS) out.put(k, or(f, k, ""));
        return out;
    }

    /** A stakeholder view record; f gives the fields (stakeholder and role are required). */
    public static Map<String, Object> stakeholderView(Map<String, Object> f) {
        Object w = f.get("w5h1");
        return Py.obj("stakeholder", f.get("stakeholder"), "role", f.get("role"), "status", or(f, "status", "ok"),
            "w5h1", w5h1(w instanceof Map<?, ?> ? Py.map(w) : Map.of()),
            "done_so_far", or(f, "done_so_far", ""), "missing", or(f, "missing", new ArrayList<>()),
            "devils_advocate", or(f, "devils_advocate", ""), "contribution", or(f, "contribution", ""),
            "sources", or(f, "sources", new ArrayList<>()), "error", or(f, "error", ""));
    }

    /** A juror merge record with every default filled in. */
    public static Map<String, Object> jurorMerge(Map<String, Object> f) {
        return Py.obj("synthesis_status", or(f, "synthesis_status", "full"), "merge_summary", or(f, "merge_summary", ""),
            "decision", or(f, "decision", ""), "resolved", or(f, "resolved", new ArrayList<>()),
            "open_issues", or(f, "open_issues", new ArrayList<>()), "next_steps", or(f, "next_steps", new ArrayList<>()),
            "reversals", or(f, "reversals", new ArrayList<>()), "spec_delta", or(f, "spec_delta", ""),
            "loop_recommended", or(f, "loop_recommended", false), "error", or(f, "error", ""));
    }

    public static Map<String, Object> councilRound(Object stepId, Object stepTitle, long iteration, Object question,
                                                   List<Object> views, Map<String, Object> juror) {
        return Py.obj("step_id", stepId, "step_title", stepTitle, "iteration", iteration, "question", question,
            "views", views, "juror", juror);
    }

    public static Map<String, Object> decisionNode(Object stepId, long iteration, List<Object> branches, Object decision,
                                                   List<Object> nextSteps) {
        return Py.obj("step_id", stepId, "iteration", iteration, "branches", branches, "decision", decision, "next_steps", nextSteps);
    }

    /** A council result record with every default filled in. */
    public static Map<String, Object> councilResult(Map<String, Object> f) {
        return Py.obj("schema", or(f, "schema", "smartsavant.council/1"), "idea", or(f, "idea", ""),
            "adapter", or(f, "adapter", ""), "synthetic", or(f, "synthetic", false),
            "stakeholders", or(f, "stakeholders", new ArrayList<>()), "sources", or(f, "sources", new ArrayList<>()),
            "rounds", or(f, "rounds", new ArrayList<>()), "decision_tree", or(f, "decision_tree", new ArrayList<>()),
            "locked_decisions", or(f, "locked_decisions", new ArrayList<>()), "flags", or(f, "flags", new ArrayList<>()));
    }

    /** A council.json object (check it with {@link Schema#checkCouncil} first) rebuilt with every default filled in. */
    public static Map<String, Object> resultFromDict(Map<String, Object> d) {
        Map<String, Object> copy = Py.map(Py.clone(d));
        List<Object> rounds = new ArrayList<>();
        for (Object r : Py.list(or(copy, "rounds", List.of()))) {
            List<Object> views = new ArrayList<>();
            for (Object v : Py.list(or(Py.map(r), "views", List.of()))) views.add(stakeholderView(Py.map(v)));
            Object j = Py.get(r, "juror");
            rounds.add(councilRound(Py.get(r, "step_id"), Py.get(r, "step_title"), ((Number) Py.get(r, "iteration")).longValue(),
                Py.get(r, "question"), views, jurorMerge(j instanceof Map<?, ?> ? Py.map(j) : Map.of())));
        }
        List<Object> tree = new ArrayList<>();
        for (Object n : Py.list(or(copy, "decision_tree", List.of()))) {
            Map<String, Object> m = Py.map(n);
            tree.add(decisionNode(m.get("step_id"), ((Number) m.get("iteration")).longValue(), Py.list(or(m, "branches", new ArrayList<>())),
                or(m, "decision", ""), Py.list(or(m, "next_steps", new ArrayList<>()))));
        }
        copy.put("rounds", rounds);
        copy.put("decision_tree", tree);
        return councilResult(copy);
    }
}
