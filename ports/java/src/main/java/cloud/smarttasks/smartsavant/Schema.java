package cloud.smarttasks.smartsavant;

import java.math.BigInteger;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * Validation of what an adapter returns, and of council files. The error strings and their order are part
 * of the contract (spec/conformance/schema.json).
 */
public final class Schema {
    private Schema() {}

    public static final int MAX_STR = 8000;
    public static final int MAX_DELTA = 20000;
    public static final int MAX_ITEMS = 20;
    public static final int MAX_ITEM = 2000;
    public static final int MAX_DEPTH = 32;
    /** Code points of all strings in one answer, keys included. */
    public static final int MAX_TOTAL = 1_000_000;
    public static final Pattern SOURCE_ID = Pattern.compile("[A-Za-z0-9._:/#@-]{1,200}");
    public static final List<String> W5H1_KEYS = List.of("who", "what", "where", "when", "why", "how");
    public static final List<String> VIEW_KEYS = List.of("w5h1", "done_so_far", "missing", "devils_advocate", "contribution", "sources");
    public static final List<String> MERGE_KEYS = List.of("merge_summary", "decision", "resolved", "open_issues", "next_steps",
        "reversals", "spec_delta", "loop_recommended");
    private static final List<String> MERGE_REQUIRED = MERGE_KEYS.stream().filter(k -> !k.equals("reversals")).toList();

    /** An answer that is not plain JSON data (schema.NotPlainData in Python). */
    public static final class NotPlainData extends IllegalArgumentException {
        private static final long serialVersionUID = 1L;
        public NotPlainData(String message) { super(message); }
    }

    /**
     * A copy of an adapter answer made only of null, Boolean, Long (or BigInteger), Double, String, List and
     * Map with String keys, built before anything else, so that what is validated is exactly what is used.
     * Integer, Short and Byte become Long; Float becomes Double. Other types, keys that are not strings,
     * nesting deeper than 32 and more than 1,000,000 code points of text throw NotPlainData.
     */
    public static Object plain(Object obj) {
        int[] budget = {MAX_TOTAL};
        return plainGo(obj, 0, budget);
    }

    private static Object plainGo(Object x, int depth, int[] budget) {
        if (depth > MAX_DEPTH) throw new NotPlainData("nested deeper than 32 levels");
        if (x == null || x instanceof Boolean || x instanceof Long || x instanceof Double || x instanceof BigInteger) return x;
        if (x instanceof Integer || x instanceof Short || x instanceof Byte) return ((Number) x).longValue();
        if (x instanceof Float f) return f.doubleValue();
        if (x instanceof String s) {
            budget[0] -= Py.len(s);
            if (budget[0] < 0) throw new NotPlainData("more than 1,000,000 characters of text");
            return s;
        }
        if (x instanceof List<?> l) {
            List<Object> out = new ArrayList<>(l.size());
            for (Object v : l.toArray()) out.add(plainGo(v, depth + 1, budget));
            return out;
        }
        if (x instanceof Map<?, ?> m) {
            Map<String, Object> out = new LinkedHashMap<>();
            for (Map.Entry<?, ?> e : List.copyOf(m.entrySet())) {
                if (!(e.getKey() instanceof String k)) throw new NotPlainData("object keys must be strings");
                budget[0] -= Py.len(k);
                out.put(k, plainGo(e.getValue(), depth + 1, budget));
            }
            if (budget[0] < 0) throw new NotPlainData("more than 1,000,000 characters of text");
            return out;
        }
        throw new NotPlainData(x.getClass().getSimpleName() + " is not JSON data");
    }

    /** An unknown key as shown in an error: control characters dropped, unpaired
     *  surrogates as U+FFFD, then at most 40 code points. */
    private static String showKey(String k) {
        StringBuilder b = new StringBuilder();
        // per code point, so that two lone surrogates around a dropped character never become a pair;
        // error text is recorded, so it must stay encodable
        k.codePoints().filter(c -> !(c < 32 || (c >= 127 && c < 160)))
            .map(c -> c <= 0xFFFF && Character.isSurrogate((char) c) ? 0xFFFD : c).forEach(b::appendCodePoint);
        String s = b.toString();
        return Py.head(s, 40) + (Py.len(s) > 40 ? "..." : "");
    }

    private static void checkStr(List<String> errs, String where, Object v, int limit, boolean nonempty) {
        if (!(v instanceof String s)) errs.add(where + " must be a string");
        else if (Py.len(s) > limit) errs.add(where + " is longer than " + limit + " characters");
        else if (s.indexOf('\0') >= 0) errs.add(where + " contains a NUL character");
        else if (Py.hasLone(s)) errs.add(where + " contains an unpaired surrogate");
        else if (nonempty && Gates.blank(s)) errs.add(where + " must not be empty");
    }

    private static void checkList(List<String> errs, String where, Object v, Pattern pattern) {
        if (!(v instanceof List<?> l)) {
            errs.add(where + " must be a list of strings");
            return;
        }
        if (l.size() > MAX_ITEMS) errs.add(where + " has more than " + MAX_ITEMS + " items");
        for (int i = 0; i < Math.min(l.size(), MAX_ITEMS + 1); i++) {
            Object x = l.get(i);
            if (Gates.blank(x)) errs.add(where + "[" + i + "] must be a non-empty string");
            else if (Py.len((String) x) > MAX_ITEM) errs.add(where + "[" + i + "] is longer than " + MAX_ITEM + " characters");
            else if (pattern != null && !pattern.matcher((String) x).matches()) errs.add(where + "[" + i + "] is not a valid source id");
            else if (((String) x).indexOf('\0') >= 0) errs.add(where + "[" + i + "] contains a NUL character");
            else if (Py.hasLone((String) x)) errs.add(where + "[" + i + "] contains an unpaired surrogate");
        }
    }

    private static boolean unknownAndMissing(List<String> errs, Map<String, Object> o, List<String> allowed, List<String> required) {
        List<String> extra = new ArrayList<>();
        for (String k : o.keySet()) if (!allowed.contains(k)) extra.add(k);
        extra = Py.sorted(extra);
        if (!extra.isEmpty()) {
            List<String> shown = new ArrayList<>();
            for (String k : extra.subList(0, Math.min(5, extra.size()))) shown.add(showKey(k));
            errs.add("unknown keys (" + extra.size() + "): " + String.join(", ", shown));
        }
        List<String> missing = new ArrayList<>();
        for (String k : required) if (!o.containsKey(k)) missing.add(k);
        missing = Py.sorted(missing);
        if (!missing.isEmpty()) errs.add("missing keys: " + String.join(", ", missing));
        return !missing.isEmpty();
    }

    /** Empty when v is a valid view; otherwise the reasons, first problem first. */
    public static List<String> checkView(Object v) {
        List<String> errs = new ArrayList<>();
        if (!(v instanceof Map<?, ?>)) return new ArrayList<>(List.of("a view must be a JSON object"));
        Map<String, Object> o = Py.map(v);
        if (unknownAndMissing(errs, o, VIEW_KEYS, VIEW_KEYS)) return errs;
        Object w = o.get("w5h1");
        if (!(w instanceof Map<?, ?> wm) || wm.size() != W5H1_KEYS.size() || !wm.keySet().containsAll(W5H1_KEYS)) {
            errs.add("w5h1 must be an object with exactly who, what, where, when, why, how");
        } else {
            for (String k : W5H1_KEYS) checkStr(errs, "w5h1." + k, wm.get(k), MAX_STR, false);
        }
        checkStr(errs, "done_so_far", o.get("done_so_far"), MAX_STR, false);
        if (o.get("missing") instanceof String) checkStr(errs, "missing", o.get("missing"), MAX_STR, false);
        else checkList(errs, "missing", o.get("missing"), null);
        checkStr(errs, "devils_advocate", o.get("devils_advocate"), MAX_STR, true);
        checkStr(errs, "contribution", o.get("contribution"), MAX_STR, false);
        checkList(errs, "sources", o.get("sources"), SOURCE_ID);
        return errs;
    }

    /** Empty when m is a valid merge; otherwise the reasons, first problem first. */
    public static List<String> checkMerge(Object m) {
        List<String> errs = new ArrayList<>();
        if (!(m instanceof Map<?, ?>)) return new ArrayList<>(List.of("a merge must be a JSON object"));
        Map<String, Object> o = Py.map(m);
        if (unknownAndMissing(errs, o, MERGE_KEYS, MERGE_REQUIRED)) return errs;
        checkStr(errs, "merge_summary", o.get("merge_summary"), MAX_STR, true);
        checkStr(errs, "decision", o.get("decision"), MAX_STR, false);
        for (String k : List.of("resolved", "open_issues", "next_steps")) checkList(errs, k, o.get(k), null);
        if (o.containsKey("reversals")) checkList(errs, "reversals", o.get("reversals"), null);
        checkStr(errs, "spec_delta", o.get("spec_delta"), MAX_DELTA, false);
        if (!(o.get("loop_recommended") instanceof Boolean)) errs.add("loop_recommended must be true or false");
        return errs;
    }

    /** A valid view with missing always a list (a shallow copy). */
    public static Map<String, Object> normalizeView(Map<String, Object> v) {
        Map<String, Object> out = new LinkedHashMap<>(v);
        if (out.get("missing") instanceof String s) out.put("missing", Gates.blank(s) ? new ArrayList<>() : new ArrayList<>(List.of(s)));
        return out;
    }

    // ---------------------------------------------------------------- council files
    private static final List<String> STATUS_V = List.of("ok", "failed", "offline");
    private static final List<String> STATUS_J = List.of("full", "thin", "recovered", "failed", "offline", "no_views");
    private static final List<String> TOP = List.of("schema", "idea", "adapter", "synthetic", "stakeholders", "sources", "rounds",
        "decision_tree", "locked_decisions", "flags");
    private static final String[][] VIEW_REC = {{"stakeholder", "s"}, {"role", "s"}, {"status", "s"}, {"w5h1", "w"},
        {"done_so_far", "s"}, {"missing", "l"}, {"devils_advocate", "s"}, {"contribution", "s"}, {"sources", "l"}, {"error", "s"}};
    private static final String[][] JUROR_REC = {{"synthesis_status", "s"}, {"merge_summary", "s"}, {"decision", "s"},
        {"resolved", "l"}, {"open_issues", "l"}, {"next_steps", "l"}, {"reversals", "l"}, {"spec_delta", "s"},
        {"loop_recommended", "b"}, {"error", "s"}};
    private static final String[][] ROUND_REC = {{"step_id", "s"}, {"step_title", "s"}, {"iteration", "i"}, {"question", "s"},
        {"views", "L"}, {"juror", "J"}};
    private static final String[][] NODE_REC = {{"step_id", "s"}, {"iteration", "i"}, {"branches", "l"}, {"decision", "s"},
        {"next_steps", "l"}};

    private static boolean goodStr(Object x) { return x instanceof String s && !Py.hasLone(s); }

    private static boolean goodList(Object v) {
        if (!(v instanceof List<?> l)) return false;
        for (Object x : l) if (!goodStr(x)) return false;
        return true;
    }

    private static boolean sameKeys(Object o, List<String> keys) {
        return o instanceof Map<?, ?> m && m.size() == keys.size() && m.keySet().containsAll(keys);
    }

    private static boolean typeOk(String t, Object v) {
        switch (t) {
            case "s": return goodStr(v);
            case "i": return Py.isInt(v) && Py.bigInt(v).signum() > 0;
            case "b": return v instanceof Boolean;
            case "l": return goodList(v);
            case "w":
                if (!sameKeys(v, W5H1_KEYS)) return false;
                for (Object x : Py.map(v).values()) if (!goodStr(x)) return false;
                return true;
            case "L": return v instanceof List;
            default: return v instanceof Map;
        }
    }

    private static void typed(List<String> errs, String where, Object d, String[][] spec) {
        List<String> keys = new ArrayList<>();
        for (String[] s : spec) keys.add(s[0]);
        if (!sameKeys(d, keys)) {
            errs.add(where + " must have exactly the keys " + String.join(", ", keys));
            return;
        }
        for (String[] s : spec) if (!typeOk(s[1], Py.map(d).get(s[0]))) errs.add(where + "." + s[0] + " has the wrong type");
    }

    /** A council.json written by an engine of this package: exact keys and JSON types everywhere, known
     *  statuses, no unpaired surrogates. At most 50 errors. */
    public static List<String> checkCouncil(Object doc) {
        List<String> errs = new ArrayList<>();
        if (!(doc instanceof Map<?, ?>)) return List.of("a council file must be a JSON object");
        Map<String, Object> d = Py.map(doc);
        if (!"smartsavant.council/1".equals(d.get("schema"))) return List.of("schema must be \"smartsavant.council/1\"");
        if (!sameKeys(d, TOP)) return List.of("top-level keys must be exactly " + String.join(", ", Py.sorted(TOP)));
        for (String k : List.of("idea", "adapter")) if (!goodStr(d.get(k))) errs.add(k + " must be a string");
        if (!(d.get("synthetic") instanceof Boolean)) errs.add("synthetic must be true or false");
        for (String k : List.of("stakeholders", "sources", "locked_decisions")) {
            if (!goodList(d.get(k))) errs.add(k + " must be a list of strings");
        }
        if (!(d.get("rounds") instanceof List<?> rounds) || !(d.get("decision_tree") instanceof List<?> tree)
            || !(d.get("flags") instanceof List<?> flags)) {
            errs.add("rounds, decision_tree and flags must be lists");
            return errs;
        }
        if (tree.size() != rounds.size()) errs.add("decision_tree must have one node per round");
        for (int i = 0; i < rounds.size(); i++) {
            String w = "rounds[" + i + "]";
            Object r = rounds.get(i);
            typed(errs, w, r, ROUND_REC);
            if (!(r instanceof Map<?, ?>) || !(Py.get(r, "views") instanceof List<?> views) || !(Py.get(r, "juror") instanceof Map<?, ?> juror)) continue;
            for (int j = 0; j < views.size(); j++) {
                Object v = views.get(j);
                typed(errs, w + ".views[" + j + "]", v, VIEW_REC);
                if (v instanceof Map<?, ?> && !STATUS_V.contains(Py.get(v, "status"))) {
                    errs.add(w + ".views[" + j + "].status must be one of " + String.join(", ", STATUS_V));
                }
            }
            typed(errs, w + ".juror", juror, JUROR_REC);
            if (!STATUS_J.contains(Py.get(juror, "synthesis_status"))) {
                errs.add(w + ".juror.synthesis_status must be one of " + String.join(", ", STATUS_J));
            }
        }
        for (int i = 0; i < tree.size(); i++) typed(errs, "decision_tree[" + i + "]", tree.get(i), NODE_REC);
        for (int i = 0; i < flags.size(); i++) {
            boolean ok = flags.get(i) instanceof Map<?, ?>;
            if (ok) for (Object x : Py.map(flags.get(i)).values()) if (!(goodStr(x) || Py.isInt(x))) ok = false;
            if (!ok) errs.add("flags[" + i + "] must be an object of strings and integers");
        }
        return errs.size() > 50 ? new ArrayList<>(errs.subList(0, 50)) : errs;
    }
}
