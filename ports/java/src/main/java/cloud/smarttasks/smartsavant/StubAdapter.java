package cloud.smarttasks.smartsavant;

import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * A deterministic adapter: no model, no network. For tests, demos and ports (the vectors pin its output).
 * It is synthetic: its views are templates, so nothing it produces is a real decision.
 * <pre>
 *   tag = first 8 hex digits of sha256(UTF-8(stakeholder + "|" + phase_id + "|" + idea)), written "sv" + tag
 *   why = the first 120 code points of the idea
 * </pre>
 */
public class StubAdapter implements LLMAdapter {
    @Override public String name() { return "stub"; }

    @Override public boolean synthetic() { return true; }

    private static String tag(String... parts) { return Canonical.sha256Hex(String.join("|", parts)).substring(0, 8); }

    @Override
    public Object view(String stakeholder, String role, String phaseId, String question, Map<String, Object> context) {
        String idea = Py.str(Py.get(context, "idea", ""));
        String t = tag(stakeholder, phaseId, idea);
        return Py.obj(
            "w5h1", Py.obj("who", stakeholder, "what", "advance " + phaseId, "where", "the target architecture",
                "when", "this iteration", "why", Py.head(idea, 120), "how", "deterministic stub sv" + t),
            "done_so_far", role + " framed " + phaseId + ".",
            "missing", new ArrayList<>(List.of(phaseId + ": target files and modules")),
            "devils_advocate", "Risk at " + phaseId + ": an unproven assumption (sv" + t + ").",
            "contribution", role + " proposes the " + phaseId + " step sv" + t + ".",
            "sources", new ArrayList<>());
    }

    @Override
    public Object merge(String phaseId, String question, List<Map<String, Object>> views, Map<String, Object> context) {
        List<Object> resolved = new ArrayList<>();
        for (int i = 0; i < views.size() && i < 5; i++) resolved.add(phaseId + " " + (i + 1) + ": " + Py.str(views.get(i).get("contribution")));
        Set<Object> issues = new LinkedHashSet<>(); // dict.fromkeys: duplicates removed, first occurrence wins
        for (Map<String, Object> v : views) issues.addAll(Py.iter(Py.get(v, "missing", List.of())));
        List<Object> open = new ArrayList<>(issues);
        return Py.obj(
            "merge_summary", "Stub juror merged " + views.size() + " view(s) for " + phaseId + ".",
            "decision", phaseId + ": proceed with the stub plan",
            "resolved", resolved,
            "open_issues", new ArrayList<>(open.subList(0, Math.min(5, open.size()))),
            "next_steps", new ArrayList<>(List.of("Write the " + phaseId + " module for the stub plan")),
            "reversals", new ArrayList<>(),
            "spec_delta", "[" + phaseId + "] stub decision recorded.",
            "loop_recommended", false);
    }
}
