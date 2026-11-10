package cloud.smarttasks.smartsavant;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The Python behaviour the reference relies on: code-point lengths and slices, {@code str()}, {@code repr()},
 * truthiness, {@code dict.get()}, code-point string order and {@code json.dumps()} (default separators,
 * {@code ensure_ascii=False}). JSON values are those of {@link Json}: null, Boolean, Long (or BigInteger),
 * Double, String, List and Map with String keys.
 */
public final class Py {
    private Py() {}

    // ------------------------------------------------------------------ types and access

    /** A Python int: Long, Integer, Short, Byte or BigInteger (never a Boolean). */
    public static boolean isInt(Object v) {
        return v instanceof Long || v instanceof Integer || v instanceof Short || v instanceof Byte || v instanceof BigInteger;
    }

    static BigInteger bigInt(Object v) {
        return v instanceof BigInteger b ? b : BigInteger.valueOf(((Number) v).longValue());
    }

    public static boolean isDict(Object v) { return v instanceof Map; }

    @SuppressWarnings("unchecked")
    public static Map<String, Object> map(Object v) { return (Map<String, Object>) v; }

    @SuppressWarnings("unchecked")
    public static List<Object> list(Object v) { return (List<Object>) v; }

    /** {@code dict.get(k, d)}: a present key with a null value gives null, not the default. */
    public static Object get(Object o, String k, Object d) {
        if (!(o instanceof Map<?, ?> m)) return d;
        return m.containsKey(k) ? m.get(k) : d;
    }

    public static Object get(Object o, String k) { return get(o, k, null); }

    public static boolean has(Object o, String k) { return o instanceof Map<?, ?> m && m.containsKey(k); }

    /** The string at key k of a map (the records this package builds). */
    public static String s(Object o, String k) { return (String) map(o).get(k); }

    /** The list at key k of a map (the records this package builds). */
    public static List<Object> l(Object o, String k) { return list(map(o).get(k)); }

    /** An ordered map from key/value pairs. */
    public static Map<String, Object> obj(Object... kv) {
        Map<String, Object> out = new LinkedHashMap<>();
        for (int k = 0; k < kv.length; k += 2) out.put((String) kv[k], kv[k + 1]);
        return out;
    }

    /** Iteration as Python does it: a string gives its code points, a list its items, a dict its keys. */
    public static List<Object> iter(Object v) {
        List<Object> out = new ArrayList<>();
        if (v instanceof String x) x.codePoints().forEach(c -> out.add(new String(Character.toChars(c))));
        else if (v instanceof List<?> x) out.addAll(x);
        else if (v instanceof Map<?, ?> x) out.addAll(x.keySet());
        return out;
    }

    /** {@code bool(v)}. */
    public static boolean truthy(Object v) {
        if (v == null) return false;
        if (v instanceof Boolean b) return b;
        if (v instanceof String x) return !x.isEmpty();
        if (isInt(v)) return bigInt(v).signum() != 0;
        if (v instanceof Number n) return n.doubleValue() != 0.0;
        if (v instanceof Collection<?> c) return !c.isEmpty();
        if (v instanceof Map<?, ?> m) return !m.isEmpty();
        return true;
    }

    /** Deep copy of JSON-like data (copy.deepcopy for what adapters see). */
    public static Object clone(Object v) {
        if (v instanceof List<?> l) {
            List<Object> out = new ArrayList<>(l.size());
            for (Object x : l) out.add(clone(x));
            return out;
        }
        if (v instanceof Map<?, ?> m) {
            Map<String, Object> out = new LinkedHashMap<>();
            for (Map.Entry<?, ?> e : m.entrySet()) out.put((String) e.getKey(), clone(e.getValue()));
            return out;
        }
        return v;
    }

    // ------------------------------------------------------------------ code points

    /** {@code len(s)} in code points (a lone surrogate counts as one). */
    public static int len(String s) { return s.codePointCount(0, s.length()); }

    /** {@code s[:n]} in code points. */
    public static String head(String s, int n) {
        if (s.length() <= n) return s;
        if (len(s) <= n) return s;
        return s.substring(0, s.offsetByCodePoints(0, n));
    }

    /** Python order of strings: by code point (Java compares UTF-16 units). */
    public static int cmp(String a, String b) {
        int i = 0;
        int j = 0;
        while (i < a.length() && j < b.length()) {
            int x = a.codePointAt(i);
            int y = b.codePointAt(j);
            if (x != y) return Integer.compare(x, y);
            i += Character.charCount(x);
            j += Character.charCount(y);
        }
        if (i < a.length()) return 1;
        return j < b.length() ? -1 : 0;
    }

    public static List<String> sorted(Collection<String> c) {
        List<String> out = new ArrayList<>(c);
        out.sort(Py::cmp);
        return out;
    }

    /** {@code str.strip(chars)}. */
    public static String strip(String s, String chars) {
        int a = 0;
        int b = s.length();
        while (a < b && chars.indexOf(s.charAt(a)) >= 0) a++;
        while (b > a && chars.indexOf(s.charAt(b - 1)) >= 0) b--;
        return s.substring(a, b);
    }

    /** True when s holds a UTF-16 surrogate without its partner (what Python sees as a lone surrogate). */
    public static boolean hasLone(String s) {
        for (int k = 0; k < s.length(); k++) {
            char c = s.charAt(k);
            if (Character.isHighSurrogate(c)) {
                if (k + 1 < s.length() && Character.isLowSurrogate(s.charAt(k + 1))) k++;
                else return true;
            } else if (Character.isLowSurrogate(c)) {
                return true;
            }
        }
        return false;
    }

    /** Every lone surrogate replaced by {@code by}. */
    public static String replaceLone(String s, String by) {
        if (!hasLone(s)) return s;
        StringBuilder b = new StringBuilder();
        for (int k = 0; k < s.length(); k++) {
            char c = s.charAt(k);
            if (Character.isHighSurrogate(c) && k + 1 < s.length() && Character.isLowSurrogate(s.charAt(k + 1))) {
                b.append(c).append(s.charAt(++k));
            } else if (Character.isSurrogate(c)) {
                b.append(by);
            } else {
                b.append(c);
            }
        }
        return b.toString();
    }

    /** {@code str.lower()}: full Unicode lower-casing (U+0130 becomes "i" plus U+0307, as in Python). */
    public static String lower(String s) { return s.toLowerCase(java.util.Locale.ROOT); }

    // ------------------------------------------------------------------ str, repr, json.dumps

    /** {@code repr(float)}: the shortest round-trip digits, fixed notation for exponents -4 to 15. */
    public static String num(double d) {
        if (Double.isNaN(d)) return "nan";
        if (Double.isInfinite(d)) return d > 0 ? "inf" : "-inf";
        if (d == 0) return (1 / d < 0) ? "-0.0" : "0.0";
        // Double.toString gives the shortest digits that round-trip (JDK 19 and later)
        BigDecimal bd = new BigDecimal(Double.toString(Math.abs(d))).stripTrailingZeros();
        if (bd.precision() == 2) { // Java prints at least two digits; Python takes one when one round-trips
            BigDecimal one = bd.round(new java.math.MathContext(1, java.math.RoundingMode.HALF_EVEN));
            if (Double.parseDouble(one.toString()) == Math.abs(d)) bd = one.stripTrailingZeros();
        }
        String digits = bd.unscaledValue().toString();
        int exp = digits.length() - 1 - bd.scale(); // decimal exponent of the first digit
        String sign = d < 0 ? "-" : "";
        if (exp >= -4 && exp < 16) {
            String plain = bd.toPlainString();
            return sign + (plain.contains(".") ? plain : plain + ".0");
        }
        String mant = digits.length() == 1 ? digits : digits.charAt(0) + "." + digits.substring(1);
        String e = String.valueOf(Math.abs(exp));
        return sign + mant + "e" + (exp < 0 ? "-" : "+") + (e.length() < 2 ? "0" + e : e);
    }

    private static boolean printable(int c) {
        if (c == ' ') return true;
        switch (Character.getType(c)) {
            case Character.CONTROL: case Character.FORMAT: case Character.SURROGATE: case Character.PRIVATE_USE:
            case Character.UNASSIGNED: case Character.LINE_SEPARATOR: case Character.PARAGRAPH_SEPARATOR:
            case Character.SPACE_SEPARATOR:
                return false;
            default:
                return true;
        }
    }

    /** {@code repr(str)}: quotes as Python picks them; non-printable characters as \xNN, \\uNNNN, \UNNNNNNNN. */
    public static String reprStr(String s) {
        char q = s.indexOf('\'') >= 0 && s.indexOf('"') < 0 ? '"' : '\'';
        StringBuilder b = new StringBuilder().append(q);
        for (int k = 0; k < s.length(); ) {
            int c = s.codePointAt(k);
            k += Character.charCount(c);
            if (c == q || c == '\\') b.append('\\').appendCodePoint(c);
            else if (c == '\n') b.append("\\n");
            else if (c == '\r') b.append("\\r");
            else if (c == '\t') b.append("\\t");
            else if (!printable(c)) {
                if (c < 0x100) b.append(String.format("\\x%02x", c));
                else if (c < 0x10000) b.append(String.format("\\u%04x", c));
                else b.append(String.format("\\U%08x", c));
            } else b.appendCodePoint(c);
        }
        return b.append(q).toString();
    }

    /** {@code repr(v)} of a JSON value. */
    public static String repr(Object v) {
        if (v == null) return "None";
        if (v instanceof Boolean b) return b ? "True" : "False";
        if (isInt(v)) return v.toString();
        if (v instanceof Number n) return num(n.doubleValue());
        if (v instanceof String x) return reprStr(x);
        if (v instanceof List<?> l) {
            StringBuilder b = new StringBuilder("[");
            for (int k = 0; k < l.size(); k++) b.append(k > 0 ? ", " : "").append(repr(l.get(k)));
            return b.append(']').toString();
        }
        if (v instanceof Map<?, ?> m) {
            StringBuilder b = new StringBuilder("{");
            boolean first = true;
            for (Map.Entry<?, ?> e : m.entrySet()) {
                b.append(first ? "" : ", ").append(repr(e.getKey())).append(": ").append(repr(e.getValue()));
                first = false;
            }
            return b.append('}').toString();
        }
        return v.toString();
    }

    /** {@code str(v)}. */
    public static String str(Object v) { return v instanceof String x ? x : repr(v); }

    private static void jsonStr(StringBuilder b, String s, boolean ensureAscii) {
        b.append('"');
        for (int k = 0; k < s.length(); k++) {
            char c = s.charAt(k);
            switch (c) {
                case '"': b.append("\\\""); break;
                case '\\': b.append("\\\\"); break;
                case '\n': b.append("\\n"); break;
                case '\r': b.append("\\r"); break;
                case '\t': b.append("\\t"); break;
                case '\b': b.append("\\b"); break;
                case '\f': b.append("\\f"); break;
                default:
                    if (c < 0x20 || (ensureAscii && c > 0x7e)) b.append(String.format("\\u%04x", (int) c));
                    else b.append(c);
            }
        }
        b.append('"');
    }

    /** {@code json.dumps(v, ensure_ascii=False, sort_keys=sortKeys)} with the default ", " and ": " separators. */
    public static String jsonDumps(Object v, boolean sortKeys) { return jsonDumps(v, sortKeys, false); }

    /** As {@link #jsonDumps(Object, boolean)}; ensureAscii writes non-ASCII as \\uXXXX, as Python's default. */
    public static String jsonDumps(Object v, boolean sortKeys, boolean ensureAscii) {
        StringBuilder b = new StringBuilder();
        dump(b, v, sortKeys, ensureAscii);
        return b.toString();
    }

    private static void dump(StringBuilder b, Object v, boolean sortKeys, boolean ensureAscii) {
        if (v == null) b.append("null");
        else if (v instanceof Boolean x) b.append(x ? "true" : "false");
        else if (isInt(v)) b.append(v.toString());
        else if (v instanceof Number n) {
            double d = n.doubleValue();
            b.append(Double.isNaN(d) ? "NaN" : Double.isInfinite(d) ? (d > 0 ? "Infinity" : "-Infinity") : num(d));
        } else if (v instanceof String x) jsonStr(b, x, ensureAscii);
        else if (v instanceof List<?> l) {
            b.append('[');
            for (int k = 0; k < l.size(); k++) {
                if (k > 0) b.append(", ");
                dump(b, l.get(k), sortKeys, ensureAscii);
            }
            b.append(']');
        } else if (v instanceof Map<?, ?> m) {
            List<String> keys = new ArrayList<>();
            for (Object k : m.keySet()) keys.add((String) k);
            if (sortKeys) keys.sort(Py::cmp);
            b.append('{');
            for (int k = 0; k < keys.size(); k++) {
                if (k > 0) b.append(", ");
                jsonStr(b, keys.get(k), ensureAscii);
                b.append(": ");
                dump(b, m.get(keys.get(k)), sortKeys, ensureAscii);
            }
            b.append('}');
        } else throw new IllegalArgumentException("Object of type " + v.getClass().getSimpleName() + " is not JSON serializable");
    }
}
