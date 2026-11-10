package cloud.smarttasks.smartsavant;

import java.math.BigInteger;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A small JSON reader with the value model the rest of the port uses. No dependencies.
 *
 * <p>Parsed values: objects are {@code LinkedHashMap<String, Object>} in document order (a repeated key keeps
 * its first position and its last value, as Python's {@code json} does), arrays {@code ArrayList<Object>},
 * integers {@code Long} (or {@code BigInteger} outside the long range), every number with a fraction or an
 * exponent {@code Double}, strings {@code String}, {@code true}/{@code false} {@code Boolean}, {@code null}
 * null. So {@code 1} and {@code 1.0} stay distinct, as in Python. {@code \\uXXXX} escapes are decoded one
 * UTF-16 unit at a time: an escaped pair becomes one code point and a lone surrogate stays a lone surrogate
 * (Python behaves the same way).
 */
public final class Json {
    /** The deepest nesting of arrays and objects the parser accepts. */
    public static final int MAX_DEPTH = 512;
    private final String s;
    private int i;
    private int depth;

    private Json(String s) { this.s = s; }

    /** Parse one JSON text; throws IllegalArgumentException on malformed input. */
    public static Object parse(String text) {
        Json p = new Json(text);
        p.ws();
        Object v = p.value();
        p.ws();
        if (p.i != p.s.length()) throw new IllegalArgumentException("trailing data at " + p.i);
        return v;
    }

    private void ws() { while (i < s.length() && " \n\r\t".indexOf(s.charAt(i)) >= 0) i++; }

    private char next() {
        if (i >= s.length()) throw new IllegalArgumentException("unexpected end of JSON");
        return s.charAt(i++);
    }

    private Object value() {
        if (i >= s.length()) throw new IllegalArgumentException("unexpected end of JSON");
        char c = s.charAt(i);
        if (c == 'n') return lit("null", null);
        if (c == 't') return lit("true", Boolean.TRUE);
        if (c == 'f') return lit("false", Boolean.FALSE);
        if (c == '"') return string();
        if (c == '[' || c == '{') {
            if (++depth > MAX_DEPTH) throw new IllegalArgumentException("JSON nested deeper than " + MAX_DEPTH + " levels");
            Object v = c == '[' ? array() : object();
            depth--;
            return v;
        }
        return number();
    }

    private Object array() {
        i++;
        List<Object> out = new ArrayList<>();
        ws();
        if (i < s.length() && s.charAt(i) == ']') { i++; return out; }
        while (true) {
            ws(); out.add(value()); ws();
            char d = next();
            if (d == ']') return out;
            if (d != ',') throw new IllegalArgumentException("bad array at " + (i - 1));
        }
    }

    private Object object() {
        i++;
        Map<String, Object> out = new LinkedHashMap<>();
        ws();
        if (i < s.length() && s.charAt(i) == '}') { i++; return out; }
        while (true) {
            ws();
            String k = string();
            ws();
            if (next() != ':') throw new IllegalArgumentException("expected : at " + (i - 1));
            ws();
            out.put(k, value());
            ws();
            char d = next();
            if (d == '}') return out;
            if (d != ',') throw new IllegalArgumentException("bad object at " + (i - 1));
        }
    }

    private Object lit(String w, Object v) {
        if (!s.startsWith(w, i)) throw new IllegalArgumentException("bad literal at " + i);
        i += w.length();
        return v;
    }

    private Object number() {
        int st = i;
        if (i < s.length() && s.charAt(i) == '-') i++;
        int digits = i;
        while (i < s.length() && Character.isDigit(s.charAt(i)) && s.charAt(i) < 128) i++;
        if (i == digits || (s.charAt(digits) == '0' && i - digits > 1)) throw new IllegalArgumentException("bad number at " + st);
        boolean frac = false;
        if (i < s.length() && s.charAt(i) == '.') {
            frac = true;
            int f = ++i;
            while (i < s.length() && s.charAt(i) >= '0' && s.charAt(i) <= '9') i++;
            if (i == f) throw new IllegalArgumentException("bad number at " + st);
        }
        if (i < s.length() && (s.charAt(i) == 'e' || s.charAt(i) == 'E')) {
            frac = true;
            i++;
            if (i < s.length() && (s.charAt(i) == '+' || s.charAt(i) == '-')) i++;
            int e = i;
            while (i < s.length() && s.charAt(i) >= '0' && s.charAt(i) <= '9') i++;
            if (i == e) throw new IllegalArgumentException("bad number at " + st);
        }
        String t = s.substring(st, i);
        if (frac) return Double.parseDouble(t);
        try {
            return Long.parseLong(t);
        } catch (NumberFormatException e) {
            return new BigInteger(t);
        }
    }

    private String string() {
        if (next() != '"') throw new IllegalArgumentException("expected a string at " + (i - 1));
        StringBuilder b = new StringBuilder();
        while (true) {
            char c = next();
            if (c == '"') return b.toString();
            if (c < 0x20) throw new IllegalArgumentException("control character in a string at " + (i - 1));
            if (c != '\\') { b.append(c); continue; }
            char e = next();
            switch (e) {
                case '"': b.append('"'); break;
                case '\\': b.append('\\'); break;
                case '/': b.append('/'); break;
                case 'b': b.append('\b'); break;
                case 'f': b.append('\f'); break;
                case 'n': b.append('\n'); break;
                case 'r': b.append('\r'); break;
                case 't': b.append('\t'); break;
                case 'u':
                    if (i + 4 > s.length()) throw new IllegalArgumentException("bad \\u escape");
                    try {
                        b.append((char) Integer.parseInt(s.substring(i, i + 4), 16));
                    } catch (NumberFormatException x) {
                        throw new IllegalArgumentException("bad \\u escape");
                    }
                    i += 4;
                    break;
                default: throw new IllegalArgumentException("bad escape");
            }
        }
    }

    /**
     * JSON value equality as the vectors define it: object key order does not matter, integers compare as
     * integers, an integer equals a float of the same value (as in Python), and {@code true} is not {@code 1}.
     */
    public static boolean same(Object a, Object b) {
        if (a == null || b == null) return a == b;
        if (a instanceof Boolean || b instanceof Boolean) return a.equals(b);
        if (a instanceof String || b instanceof String) return a.equals(b);
        if (Py.isInt(a) && Py.isInt(b)) return Py.bigInt(a).equals(Py.bigInt(b));
        if (a instanceof Number x && b instanceof Number y) return x.doubleValue() == y.doubleValue();
        if (a instanceof List<?> x && b instanceof List<?> y) {
            if (x.size() != y.size()) return false;
            for (int k = 0; k < x.size(); k++) if (!same(x.get(k), y.get(k))) return false;
            return true;
        }
        if (a instanceof Map<?, ?> x && b instanceof Map<?, ?> y) {
            if (x.size() != y.size()) return false;
            for (Map.Entry<?, ?> e : x.entrySet()) {
                if (!y.containsKey(e.getKey()) || !same(e.getValue(), y.get(e.getKey()))) return false;
            }
            return true;
        }
        return false;
    }
}
