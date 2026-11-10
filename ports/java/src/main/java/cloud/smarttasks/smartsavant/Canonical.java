package cloud.smarttasks.smartsavant;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Canonical JSON: the byte form every SmartSavant engine hashes (run digests) and compares. Keys ASCII and
 * sorted, no whitespace, integers only, and only {@code "}, {@code \} and characters below U+0020 escaped.
 */
public final class Canonical {
    private Canonical() {}

    private static void str(StringBuilder b, String x) {
        b.append('"');
        for (int k = 0; k < x.length(); k++) {
            char c = x.charAt(k);
            switch (c) {
                case '"': b.append("\\\""); break;
                case '\\': b.append("\\\\"); break;
                case '\b': b.append("\\b"); break;
                case '\f': b.append("\\f"); break;
                case '\n': b.append("\\n"); break;
                case '\r': b.append("\\r"); break;
                case '\t': b.append("\\t"); break;
                default:
                    if (c < 0x20) b.append(String.format("\\u%04x", (int) c)); else b.append(c);
            }
        }
        b.append('"');
    }

    /** The canonical JSON text of value; IllegalArgumentException for floats, non-ASCII keys and other types. */
    public static String dumps(Object v) {
        StringBuilder b = new StringBuilder();
        canon(b, v);
        return b.toString();
    }

    /** The UTF-8 bytes of {@link #dumps}. */
    public static byte[] encode(Object v) { return dumps(v).getBytes(StandardCharsets.UTF_8); }

    private static void canon(StringBuilder b, Object v) {
        if (v == null) b.append("null");
        else if (v instanceof Boolean x) b.append(x ? "true" : "false");
        else if (Py.isInt(v)) b.append(v.toString());
        else if (v instanceof String x) str(b, x);
        else if (v instanceof List<?> l) {
            b.append('[');
            for (int k = 0; k < l.size(); k++) {
                if (k > 0) b.append(',');
                canon(b, l.get(k));
            }
            b.append(']');
        } else if (v instanceof Map<?, ?> m) {
            List<String> keys = new ArrayList<>();
            for (Object k : m.keySet()) {
                if (!(k instanceof String ks) || !ks.chars().allMatch(c -> c < 0x80)) {
                    throw new IllegalArgumentException("canonical JSON object keys must be ASCII strings");
                }
                keys.add(ks);
            }
            keys.sort(null);
            b.append('{');
            for (int k = 0; k < keys.size(); k++) {
                if (k > 0) b.append(',');
                str(b, keys.get(k));
                b.append(':');
                canon(b, m.get(keys.get(k)));
            }
            b.append('}');
        } else {
            String t = v instanceof Number ? "float" : v.getClass().getSimpleName();
            throw new IllegalArgumentException("canonical JSON does not encode " + t + " (floats are not allowed)");
        }
    }

    /** Lowercase hex SHA-256 of bytes. */
    public static String sha256Hex(byte[] data) {
        try {
            byte[] h = MessageDigest.getInstance("SHA-256").digest(data);
            StringBuilder b = new StringBuilder();
            for (byte x : h) b.append(String.format("%02x", x & 0xff));
            return b.toString();
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    /** Lowercase hex SHA-256 of the UTF-8 bytes of text. */
    public static String sha256Hex(String text) { return sha256Hex(text.getBytes(StandardCharsets.UTF_8)); }
}
