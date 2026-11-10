using System.Collections;
using System.Globalization;
using System.Numerics;
using System.Text;

namespace SmartSavant;

/// <summary>
/// The Python behaviour the reference relies on: code-point lengths and slices, str(), repr(), truthiness,
/// dict.get(), code-point string order, str.lower() and json.dumps() (default separators,
/// ensure_ascii=False). JSON values are those of <see cref="Json"/>: null, bool, long (or BigInteger),
/// double, string, <c>List&lt;object?&gt;</c> and <see cref="JObj"/>.
/// </summary>
public static class Py
{
    // ------------------------------------------------------------------ types and access

    /// <summary>A Python int: long, int, short, sbyte, byte or BigInteger (never a bool).</summary>
    public static bool IsInt(object? v) => v is long or int or short or sbyte or byte or uint or ushort or BigInteger;

    public static bool IsNumber(object? v) => IsInt(v) || v is double or float;

    public static BigInteger BigInt(object v) => v switch
    {
        BigInteger b => b,
        long l => l,
        int x => x,
        short x => x,
        sbyte x => x,
        byte x => x,
        uint x => x,
        ushort x => x,
        _ => throw new ArgumentException("not an integer"),
    };

    public static double ToDouble(object v) => v switch
    {
        double d => d,
        float f => f,
        _ => (double)BigInt(v),
    };

    public static JObj M(object? v) => (JObj)v!;

    public static List<object?> L(object? v) => (List<object?>)v!;

    /// <summary>dict.get(k, d): a present key with a null value gives null, not the default.</summary>
    public static object? Get(object? o, string k, object? d = null) => o is JObj m && m.TryGetValue(k, out var v) ? v : d;

    public static bool Has(object? o, string k) => o is JObj m && m.ContainsKey(k);

    /// <summary>The string at key k of a record this package built.</summary>
    public static string S(object? o, string k) => (string)M(o)[k]!;

    /// <summary>The list at key k of a record this package built.</summary>
    public static List<object?> L(object? o, string k) => L(M(o)[k]);

    /// <summary>An ordered object from key/value pairs.</summary>
    public static JObj O(params object?[] kv)
    {
        var o = new JObj();
        for (int k = 0; k < kv.Length; k += 2) o[(string)kv[k]!] = kv[k + 1];
        return o;
    }

    /// <summary>A new list of the given items.</summary>
    public static List<object?> A(params object?[] items) => new(items);

    /// <summary>Iteration as Python does it: a string gives its code points, a list its items, a dict its keys.</summary>
    public static List<object?> Iter(object? v)
    {
        var output = new List<object?>();
        if (v is string s) output.AddRange(CodePoints(s).Select(c => (object?)c));
        else if (v is JObj o) output.AddRange(o.Keys);
        else if (v is IList l) foreach (var x in l) output.Add(x);
        return output;
    }

    /// <summary>bool(v).</summary>
    public static bool Truthy(object? v) => v switch
    {
        null => false,
        bool b => b,
        string s => s.Length > 0,
        double d => d != 0,
        float f => f != 0,
        JObj o => o.Count > 0,
        ICollection c => c.Count > 0,
        _ when IsInt(v) => !BigInt(v!).IsZero,
        _ => true,
    };

    /// <summary>Deep copy of JSON-like data (copy.deepcopy for what adapters see).</summary>
    public static object? Clone(object? v)
    {
        if (v is JObj o)
        {
            var output = new JObj();
            foreach (var kv in o) output[kv.Key] = Clone(kv.Value);
            return output;
        }
        if (v is List<object?> l) return l.Select(Clone).ToList();
        return v;
    }

    // ------------------------------------------------------------------ code points

    /// <summary>The code points of s as strings (a lone surrogate is one code point, as in Python).</summary>
    public static IEnumerable<string> CodePoints(string s)
    {
        for (int k = 0; k < s.Length; k++)
        {
            if (char.IsHighSurrogate(s[k]) && k + 1 < s.Length && char.IsLowSurrogate(s[k + 1]))
            {
                yield return s.Substring(k, 2);
                k++;
            }
            else yield return s[k].ToString();
        }
    }

    private static int CodePointAt(string s, int k) =>
        char.IsHighSurrogate(s[k]) && k + 1 < s.Length && char.IsLowSurrogate(s[k + 1]) ? char.ConvertToUtf32(s[k], s[k + 1]) : s[k];

    /// <summary>len(s) in code points.</summary>
    public static int Len(string s)
    {
        int n = 0;
        for (int k = 0; k < s.Length; k++, n++)
        {
            if (char.IsHighSurrogate(s[k]) && k + 1 < s.Length && char.IsLowSurrogate(s[k + 1])) k++;
        }
        return n;
    }

    /// <summary>s[:n] in code points.</summary>
    public static string Head(string s, int n)
    {
        if (s.Length <= n) return s;
        int k = 0;
        for (int c = 0; c < n && k < s.Length; c++)
        {
            k += char.IsHighSurrogate(s[k]) && k + 1 < s.Length && char.IsLowSurrogate(s[k + 1]) ? 2 : 1;
        }
        return s.Substring(0, k);
    }

    /// <summary>Python order of strings: by code point (string.CompareOrdinal compares UTF-16 units).</summary>
    public static int Cmp(string? a, string? b)
    {
        a ??= "";
        b ??= "";
        int i = 0, j = 0;
        while (i < a.Length && j < b.Length)
        {
            int x = CodePointAt(a, i), y = CodePointAt(b, j);
            if (x != y) return x.CompareTo(y);
            i += x > 0xFFFF ? 2 : 1;
            j += y > 0xFFFF ? 2 : 1;
        }
        if (i < a.Length) return 1;
        return j < b.Length ? -1 : 0;
    }

    public static readonly IComparer<string> Order = Comparer<string>.Create((a, b) => Cmp(a, b));

    public static List<string> Sorted(IEnumerable<string> items)
    {
        var output = items.ToList();
        output.Sort(Order);
        return output;
    }

    /// <summary>str.strip(chars).</summary>
    public static string Strip(string s, string chars)
    {
        int a = 0, b = s.Length;
        while (a < b && chars.IndexOf(s[a]) >= 0) a++;
        while (b > a && chars.IndexOf(s[b - 1]) >= 0) b--;
        return s.Substring(a, b - a);
    }

    /// <summary>True when s holds a UTF-16 surrogate without its partner (a lone surrogate in Python).</summary>
    public static bool HasLone(string s)
    {
        for (int k = 0; k < s.Length; k++)
        {
            char c = s[k];
            if (char.IsHighSurrogate(c))
            {
                if (k + 1 < s.Length && char.IsLowSurrogate(s[k + 1])) k++;
                else return true;
            }
            else if (char.IsLowSurrogate(c)) return true;
        }
        return false;
    }

    /// <summary>Every lone surrogate replaced by <paramref name="by"/>.</summary>
    public static string ReplaceLone(string s, string by)
    {
        if (!HasLone(s)) return s;
        var b = new StringBuilder();
        foreach (var cp in CodePoints(s)) b.Append(cp.Length == 1 && char.IsSurrogate(cp[0]) ? by : cp);
        return b.ToString();
    }

    /// <summary>str.lower(): Unicode lower-casing, with U+0130 as "i" plus U+0307 as Python writes it.</summary>
    public static string Lower(string s) => s.Replace("\u0130", "i\u0307").ToLowerInvariant();

    // ------------------------------------------------------------------ str, repr, json.dumps

    /// <summary>repr(float): the shortest round-trip digits, fixed notation for exponents -4 to 15.</summary>
    public static string Num(double d)
    {
        if (double.IsNaN(d)) return "nan";
        if (double.IsInfinity(d)) return d > 0 ? "inf" : "-inf";
        if (d == 0) return double.IsNegative(d) ? "-0.0" : "0.0";
        // "R" is the shortest round-trip form on .NET Core 3.0 and later, e.g. "1.5E-05" or "123.456"
        string r = Math.Abs(d).ToString("R", CultureInfo.InvariantCulture);
        int e = 0;
        int ei = r.IndexOfAny(new[] { 'E', 'e' });
        if (ei >= 0)
        {
            e = int.Parse(r.Substring(ei + 1), NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture);
            r = r.Substring(0, ei);
        }
        int dot = r.IndexOf('.');
        string intPart = dot >= 0 ? r.Substring(0, dot) : r;
        string fracPart = dot >= 0 ? r.Substring(dot + 1) : "";
        string all = intPart + fracPart;
        int lead = 0;
        while (lead < all.Length - 1 && all[lead] == '0') lead++;
        string digits = all.Substring(lead).TrimEnd('0');
        if (digits.Length == 0) digits = "0";
        int exp = intPart.Length - 1 - lead + e; // decimal exponent of the first digit
        string sign = d < 0 ? "-" : "";
        if (exp >= -4 && exp < 16)
        {
            string text;
            if (exp >= 0)
            {
                string ip = digits.Length > exp + 1 ? digits.Substring(0, exp + 1) : digits.PadRight(exp + 1, '0');
                string fp = digits.Length > exp + 1 ? digits.Substring(exp + 1) : "0";
                text = ip + "." + fp;
            }
            else text = "0." + new string('0', -exp - 1) + digits;
            return sign + text;
        }
        string mant = digits.Length == 1 ? digits : digits[0] + "." + digits.Substring(1);
        string es = Math.Abs(exp).ToString(CultureInfo.InvariantCulture);
        return sign + mant + "e" + (exp < 0 ? "-" : "+") + (es.Length < 2 ? "0" + es : es);
    }

    private static bool Printable(int c)
    {
        if (c == ' ') return true;
        switch (CharUnicodeInfo.GetUnicodeCategory(c))
        {
            case UnicodeCategory.Control:
            case UnicodeCategory.Format:
            case UnicodeCategory.Surrogate:
            case UnicodeCategory.PrivateUse:
            case UnicodeCategory.OtherNotAssigned:
            case UnicodeCategory.LineSeparator:
            case UnicodeCategory.ParagraphSeparator:
            case UnicodeCategory.SpaceSeparator:
                return false;
            default:
                return true;
        }
    }

    /// <summary>repr(str): quotes as Python picks them; non-printable characters as \xNN, \uNNNN, \UNNNNNNNN.</summary>
    public static string ReprStr(string s)
    {
        char q = s.Contains('\'') && !s.Contains('"') ? '"' : '\'';
        var b = new StringBuilder().Append(q);
        foreach (var cp in CodePoints(s))
        {
            int c = cp.Length == 2 ? char.ConvertToUtf32(cp[0], cp[1]) : cp[0];
            if (c == q || c == '\\') b.Append('\\').Append(cp);
            else if (c == '\n') b.Append("\\n");
            else if (c == '\r') b.Append("\\r");
            else if (c == '\t') b.Append("\\t");
            else if (!Printable(c))
            {
                if (c < 0x100) b.Append("\\x").Append(c.ToString("x2"));
                else if (c < 0x10000) b.Append("\\u").Append(c.ToString("x4"));
                else b.Append("\\U").Append(c.ToString("x8"));
            }
            else b.Append(cp);
        }
        return b.Append(q).ToString();
    }

    /// <summary>repr(v) of a JSON value.</summary>
    public static string Repr(object? v)
    {
        switch (v)
        {
            case null: return "None";
            case bool b: return b ? "True" : "False";
            case string s: return ReprStr(s);
            case double or float: return Num(ToDouble(v));
            case JObj o: return "{" + string.Join(", ", o.Select(kv => ReprStr(kv.Key) + ": " + Repr(kv.Value))) + "}";
            case IList l: return "[" + string.Join(", ", l.Cast<object?>().Select(Repr)) + "]";
        }
        if (IsInt(v)) return BigInt(v).ToString(CultureInfo.InvariantCulture);
        return v.ToString() ?? "";
    }

    /// <summary>str(v).</summary>
    public static string Str(object? v) => v is string s ? s : Repr(v);

    private static void JsonStr(StringBuilder b, string s, bool ensureAscii)
    {
        b.Append('"');
        foreach (char c in s)
        {
            switch (c)
            {
                case '"': b.Append("\\\""); break;
                case '\\': b.Append("\\\\"); break;
                case '\n': b.Append("\\n"); break;
                case '\r': b.Append("\\r"); break;
                case '\t': b.Append("\\t"); break;
                case '\b': b.Append("\\b"); break;
                case '\f': b.Append("\\f"); break;
                default:
                    if (c < 0x20 || (ensureAscii && c > 0x7e)) b.Append("\\u").Append(((int)c).ToString("x4"));
                    else b.Append(c);
                    break;
            }
        }
        b.Append('"');
    }

    /// <summary>json.dumps(v, ensure_ascii=False, sort_keys=sortKeys) with the default ", " and ": " separators;
    /// ensureAscii writes non-ASCII as \uXXXX, as Python's default.</summary>
    public static string JsonDumps(object? v, bool sortKeys, bool ensureAscii = false)
    {
        var b = new StringBuilder();
        Dump(b, v, sortKeys, ensureAscii);
        return b.ToString();
    }

    private static void Dump(StringBuilder b, object? v, bool sortKeys, bool ensureAscii)
    {
        switch (v)
        {
            case null: b.Append("null"); return;
            case bool x: b.Append(x ? "true" : "false"); return;
            case string s: JsonStr(b, s, ensureAscii); return;
            case double or float:
                double d = ToDouble(v);
                b.Append(double.IsNaN(d) ? "NaN" : double.IsInfinity(d) ? (d > 0 ? "Infinity" : "-Infinity") : Num(d));
                return;
            case JObj o:
                var keys = o.Keys.ToList();
                if (sortKeys) keys.Sort(Order);
                b.Append('{');
                for (int k = 0; k < keys.Count; k++)
                {
                    if (k > 0) b.Append(", ");
                    JsonStr(b, keys[k], ensureAscii);
                    b.Append(": ");
                    Dump(b, o[keys[k]], sortKeys, ensureAscii);
                }
                b.Append('}');
                return;
            case IList l:
                b.Append('[');
                for (int k = 0; k < l.Count; k++)
                {
                    if (k > 0) b.Append(", ");
                    Dump(b, l[k], sortKeys, ensureAscii);
                }
                b.Append(']');
                return;
        }
        if (IsInt(v)) { b.Append(BigInt(v).ToString(CultureInfo.InvariantCulture)); return; }
        throw new ArgumentException($"Object of type {v.GetType().Name} is not JSON serializable");
    }
}
