package cloud.smarttasks.smartsavant;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Replays the answers of a recorded run (a council.json written by an engine of this package). Views and
 * merges whose status was not ok / full / thin are replayed as failures (the adapter throws ReplayError),
 * so the replayed run has the same health as the original.
 */
public class ReplayAdapter implements LLMAdapter {
    private final boolean synthetic;
    private final Map<List<Object>, Object> views = new HashMap<>();
    private final Map<List<Object>, Object> merges = new HashMap<>();

    /** recorded: a parsed council object. */
    public ReplayAdapter(Map<String, Object> recorded) {
        this.synthetic = Py.truthy(Py.get(recorded, "synthetic", false));
        for (Object r : Py.iter(Py.get(recorded, "rounds", List.of()))) {
            Object step = Py.get(r, "step_id");
            Object it = key(Py.get(r, "iteration"));
            for (Object v : Py.iter(Py.get(r, "views", List.of()))) views.putIfAbsent(Arrays.asList(step, it, Py.get(v, "stakeholder")), v);
            merges.put(Arrays.asList(step, it), Py.get(r, "juror", Map.of()));
        }
    }

    /** Reads a council.json file. */
    public static ReplayAdapter fromFile(Path file) throws IOException {
        return new ReplayAdapter(Py.map(Json.parse(Files.readString(file, StandardCharsets.UTF_8))));
    }

    private static Object key(Object it) { return Py.isInt(it) ? (Object) Py.bigInt(it) : it; }

    @Override public String name() { return "replay"; }

    @Override public boolean synthetic() { return synthetic; }

    @Override
    public Object view(String stakeholder, String role, String phaseId, String question, Map<String, Object> context) {
        Object v = views.get(Arrays.asList(phaseId, key(Py.get(context, "iteration", 1L)), stakeholder));
        if (v == null || !"ok".equals(Py.get(v, "status"))) {
            throw new ReplayError("no recorded valid view for " + stakeholder + " at " + phaseId);
        }
        return Py.clone(Py.obj("w5h1", Py.get(v, "w5h1"), "done_so_far", Py.get(v, "done_so_far"), "missing", Py.get(v, "missing"),
            "devils_advocate", Py.get(v, "devils_advocate"), "contribution", Py.get(v, "contribution"), "sources", Py.get(v, "sources")));
    }

    @Override
    public Object merge(String phaseId, String question, List<Map<String, Object>> views, Map<String, Object> context) {
        Object m = merges.get(Arrays.asList(phaseId, key(Py.get(context, "iteration", 1L))));
        Object st = Py.get(m, "synthesis_status");
        if (m == null || !("full".equals(st) || "thin".equals(st))) throw new ReplayError("no recorded valid merge for " + phaseId);
        Map<String, Object> out = Py.obj();
        for (String k : Schema.MERGE_KEYS) out.put(k, Py.get(m, k));
        return Py.clone(out);
    }
}
