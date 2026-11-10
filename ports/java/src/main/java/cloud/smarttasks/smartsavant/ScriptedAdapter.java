package cloud.smarttasks.smartsavant;

import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * An adapter that answers from a script: for tests, the conformance vectors and ports.
 * <pre>
 *   {"views":  {"frame|1|Developer": [ANSWER, ...]},     one entry per call, in order
 *    "merges": {"build|1": [{"raise": "timeout"}, ANSWER]}}
 * </pre>
 * A key not in the script is answered by the stub. An entry {"raise": MESSAGE} throws AdapterError(MESSAGE);
 * anything else is returned as the answer (valid or not: the engine checks). After the list for a key is
 * used up, its last entry repeats. synthetic is true.
 */
public class ScriptedAdapter extends StubAdapter {
    private final Map<String, Object> views;
    private final Map<String, Object> merges;
    private final Map<String, Integer> calls = new HashMap<>();

    /** script: {"views": {...}, "merges": {...}} as parsed JSON (null for none). */
    public ScriptedAdapter(Object script) {
        Object v = Py.get(script, "views", Map.of());
        Object m = Py.get(script, "merges", Map.of());
        this.views = v instanceof Map<?, ?> ? new LinkedHashMap<>(Py.map(v)) : new LinkedHashMap<>();
        this.merges = m instanceof Map<?, ?> ? new LinkedHashMap<>(Py.map(m)) : new LinkedHashMap<>();
    }

    @Override public String name() { return "scripted"; }

    @Override public boolean synthetic() { return true; }

    private Object next(Map<String, Object> table, String key) {
        Object seq = table.get(key);
        if (!Py.truthy(seq)) return null;
        List<Object> items = Py.iter(seq);
        int i = calls.getOrDefault(key, 0);
        calls.put(key, i + 1);
        Object ans = items.get(Math.min(i, items.size() - 1));
        if (ans instanceof Map<?, ?> a && a.size() == 1 && a.containsKey("raise")) throw new AdapterError(Py.str(a.get("raise")));
        return Py.clone(ans);
    }

    @Override
    public Object view(String stakeholder, String role, String phaseId, String question, Map<String, Object> context) {
        String key = phaseId + "|" + Py.str(Py.get(context, "iteration", 1L)) + "|" + stakeholder;
        if (views.containsKey(key)) return next(views, key);
        return super.view(stakeholder, role, phaseId, question, context);
    }

    @Override
    public Object merge(String phaseId, String question, List<Map<String, Object>> views, Map<String, Object> context) {
        String key = phaseId + "|" + Py.str(Py.get(context, "iteration", 1L));
        if (merges.containsKey(key)) return next(merges, key);
        return super.merge(phaseId, question, views, context);
    }
}
