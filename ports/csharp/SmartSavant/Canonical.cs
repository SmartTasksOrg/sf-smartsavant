using System.Collections;
using System.Globalization;
using System.Security.Cryptography;
using System.Text;

namespace SmartSavant;

/// <summary>
/// Canonical JSON: the byte form every SmartSavant engine hashes (run digests) and compares. Keys ASCII and
/// sorted, no whitespace, integers only, and only <c>"</c>, <c>\</c> and characters below U+0020 escaped.
/// </summary>
public static class Canonical
{
    private static void Str(StringBuilder b, string x)
    {
        b.Append('"');
        foreach (char c in x)
        {
            switch (c)
            {
                case '"': b.Append("\\\""); break;
                case '\\': b.Append("\\\\"); break;
                case '\b': b.Append("\\b"); break;
                case '\f': b.Append("\\f"); break;
                case '\n': b.Append("\\n"); break;
                case '\r': b.Append("\\r"); break;
                case '\t': b.Append("\\t"); break;
                default:
                    if (c < 0x20) b.Append("\\u").Append(((int)c).ToString("x4")); else b.Append(c);
                    break;
            }
        }
        b.Append('"');
    }

    /// <summary>The canonical JSON text of v; ArgumentException for floats, non-ASCII keys and other types.</summary>
    public static string Dumps(object? v)
    {
        var b = new StringBuilder();
        Canon(b, v);
        return b.ToString();
    }

    /// <summary>The UTF-8 bytes of <see cref="Dumps"/>.</summary>
    public static byte[] Encode(object? v) => Encoding.UTF8.GetBytes(Dumps(v));

    private static void Canon(StringBuilder b, object? v)
    {
        switch (v)
        {
            case null: b.Append("null"); return;
            case bool x: b.Append(x ? "true" : "false"); return;
            case string s: Str(b, s); return;
            case JObj o:
                foreach (var k in o.Keys)
                {
                    if (k.Any(c => c > 0x7f)) throw new ArgumentException("canonical JSON object keys must be ASCII strings");
                }
                var keys = o.Keys.ToList();
                keys.Sort(string.CompareOrdinal);
                b.Append('{');
                for (int k = 0; k < keys.Count; k++)
                {
                    if (k > 0) b.Append(',');
                    Str(b, keys[k]);
                    b.Append(':');
                    Canon(b, o[keys[k]]);
                }
                b.Append('}');
                return;
            case IList l:
                b.Append('[');
                for (int k = 0; k < l.Count; k++)
                {
                    if (k > 0) b.Append(',');
                    Canon(b, l[k]);
                }
                b.Append(']');
                return;
        }
        if (Py.IsInt(v)) { b.Append(Py.BigInt(v).ToString(CultureInfo.InvariantCulture)); return; }
        string t = v is double or float ? "float" : v.GetType().Name;
        throw new ArgumentException($"canonical JSON does not encode {t} (floats are not allowed)");
    }

    /// <summary>Lowercase hex SHA-256 of bytes.</summary>
    public static string Sha256Hex(byte[] data) => Convert.ToHexString(SHA256.HashData(data)).ToLowerInvariant();

    /// <summary>Lowercase hex SHA-256 of the UTF-8 bytes of text.</summary>
    public static string Sha256Hex(string text) => Sha256Hex(Encoding.UTF8.GetBytes(text));
}
