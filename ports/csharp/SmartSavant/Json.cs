using System.Collections;
using System.Globalization;
using System.Numerics;
using System.Text;

namespace SmartSavant;

/// <summary>
/// A JSON object that keeps its keys in insertion order (as a Python dict does). Setting an existing key
/// keeps its position. Reading a missing key with the indexer gives null; use <see cref="ContainsKey"/> to
/// tell a missing key from a null value.
/// </summary>
public sealed class JObj : IEnumerable<KeyValuePair<string, object?>>
{
    private readonly Dictionary<string, int> idx = new(StringComparer.Ordinal);
    private readonly List<string> keys = new();
    private readonly List<object?> vals = new();

    public int Count => keys.Count;
    public IReadOnlyList<string> Keys => keys;
    public IReadOnlyList<object?> Values => vals;

    public object? this[string k]
    {
        get => idx.TryGetValue(k, out var i) ? vals[i] : null;
        set
        {
            if (idx.TryGetValue(k, out var i)) vals[i] = value;
            else
            {
                idx[k] = keys.Count;
                keys.Add(k);
                vals.Add(value);
            }
        }
    }

    /// <summary>For collection initializers: <c>new JObj { { "a", 1L } }</c>.</summary>
    public void Add(string k, object? v) => this[k] = v;

    public bool ContainsKey(string k) => idx.ContainsKey(k);

    public bool TryGetValue(string k, out object? v)
    {
        if (idx.TryGetValue(k, out var i)) { v = vals[i]; return true; }
        v = null;
        return false;
    }

    public IEnumerator<KeyValuePair<string, object?>> GetEnumerator()
    {
        for (int i = 0; i < keys.Count; i++) yield return new KeyValuePair<string, object?>(keys[i], vals[i]);
    }

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
}

/// <summary>
/// A small JSON reader with the value model of this port, and JSON value equality. No dependencies.
/// Parsed values: objects are <see cref="JObj"/> in document order (a repeated key keeps its first position
/// and its last value, as Python's json does), arrays <c>List&lt;object?&gt;</c>, integers <c>long</c> (or
/// <see cref="BigInteger"/> outside the long range), numbers with a fraction or an exponent <c>double</c>,
/// strings, booleans and null. So 1 and 1.0 stay distinct, as in Python. <c>\uXXXX</c> escapes are decoded one
/// UTF-16 unit at a time: an escaped pair becomes one code point and a lone surrogate stays a lone surrogate
/// (System.Text.Json would refuse or replace it; Python keeps it).
/// </summary>
public sealed class Json
{
    public const int MaxDepth = 512;
    private readonly string s;
    private int i;
    private int depth;

    private Json(string s) { this.s = s; }

    /// <summary>Parse one JSON text; throws FormatException on malformed input.</summary>
    public static object? Parse(string text)
    {
        var p = new Json(text);
        p.Ws();
        var v = p.Value();
        p.Ws();
        if (p.i != p.s.Length) throw new FormatException($"trailing data at {p.i}");
        return v;
    }

    private void Ws() { while (i < s.Length && " \n\r\t".IndexOf(s[i]) >= 0) i++; }

    private char Next()
    {
        if (i >= s.Length) throw new FormatException("unexpected end of JSON");
        return s[i++];
    }

    private object? Value()
    {
        if (i >= s.Length) throw new FormatException("unexpected end of JSON");
        char c = s[i];
        if (c == 'n') return Lit("null", null);
        if (c == 't') return Lit("true", true);
        if (c == 'f') return Lit("false", false);
        if (c == '"') return Str();
        if (c == '[' || c == '{')
        {
            if (++depth > MaxDepth) throw new FormatException($"JSON nested deeper than {MaxDepth} levels");
            object v = c == '[' ? Array() : Obj();
            depth--;
            return v;
        }
        return Number();
    }

    private List<object?> Array()
    {
        i++;
        var output = new List<object?>();
        Ws();
        if (i < s.Length && s[i] == ']') { i++; return output; }
        while (true)
        {
            Ws(); output.Add(Value()); Ws();
            char d = Next();
            if (d == ']') return output;
            if (d != ',') throw new FormatException($"bad array at {i - 1}");
        }
    }

    private JObj Obj()
    {
        i++;
        var output = new JObj();
        Ws();
        if (i < s.Length && s[i] == '}') { i++; return output; }
        while (true)
        {
            Ws();
            string k = Str();
            Ws();
            if (Next() != ':') throw new FormatException($"expected : at {i - 1}");
            Ws();
            output[k] = Value();
            Ws();
            char d = Next();
            if (d == '}') return output;
            if (d != ',') throw new FormatException($"bad object at {i - 1}");
        }
    }

    private object? Lit(string w, object? v)
    {
        if (string.CompareOrdinal(s, i, w, 0, w.Length) != 0) throw new FormatException($"bad literal at {i}");
        i += w.Length;
        return v;
    }

    private static bool Digit(char c) => c >= '0' && c <= '9';

    private object Number()
    {
        int st = i;
        if (i < s.Length && s[i] == '-') i++;
        int digits = i;
        while (i < s.Length && Digit(s[i])) i++;
        if (i == digits || (s[digits] == '0' && i - digits > 1)) throw new FormatException($"bad number at {st}");
        bool frac = false;
        if (i < s.Length && s[i] == '.')
        {
            frac = true;
            int f = ++i;
            while (i < s.Length && Digit(s[i])) i++;
            if (i == f) throw new FormatException($"bad number at {st}");
        }
        if (i < s.Length && (s[i] == 'e' || s[i] == 'E'))
        {
            frac = true;
            i++;
            if (i < s.Length && (s[i] == '+' || s[i] == '-')) i++;
            int e = i;
            while (i < s.Length && Digit(s[i])) i++;
            if (i == e) throw new FormatException($"bad number at {st}");
        }
        string t = s.Substring(st, i - st);
        if (frac) return double.Parse(t, NumberStyles.Float, CultureInfo.InvariantCulture);
        if (long.TryParse(t, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var l)) return l;
        return BigInteger.Parse(t, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture);
    }

    private string Str()
    {
        if (Next() != '"') throw new FormatException($"expected a string at {i - 1}");
        var b = new StringBuilder();
        while (true)
        {
            char c = Next();
            if (c == '"') return b.ToString();
            if (c < 0x20) throw new FormatException($"control character in a string at {i - 1}");
            if (c != '\\') { b.Append(c); continue; }
            char e = Next();
            switch (e)
            {
                case '"': b.Append('"'); break;
                case '\\': b.Append('\\'); break;
                case '/': b.Append('/'); break;
                case 'b': b.Append('\b'); break;
                case 'f': b.Append('\f'); break;
                case 'n': b.Append('\n'); break;
                case 'r': b.Append('\r'); break;
                case 't': b.Append('\t'); break;
                case 'u':
                    if (i + 4 > s.Length || !int.TryParse(s.AsSpan(i, 4), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out var u))
                        throw new FormatException("bad \\u escape");
                    b.Append((char)u);
                    i += 4;
                    break;
                default: throw new FormatException("bad escape");
            }
        }
    }

    /// <summary>
    /// JSON value equality as the vectors define it: object key order does not matter, integers compare as
    /// integers, an integer equals a float of the same value (as in Python), and true is not 1.
    /// </summary>
    public static bool Same(object? a, object? b)
    {
        if (a is null || b is null) return a is null && b is null;
        if (a is bool || b is bool) return a.Equals(b);
        if (a is string || b is string) return a.Equals(b);
        if (Py.IsInt(a) && Py.IsInt(b)) return Py.BigInt(a) == Py.BigInt(b);
        if (Py.IsNumber(a) && Py.IsNumber(b)) return Py.ToDouble(a) == Py.ToDouble(b);
        if (a is IList x && b is IList y)
        {
            if (x.Count != y.Count) return false;
            for (int k = 0; k < x.Count; k++) if (!Same(x[k], y[k])) return false;
            return true;
        }
        if (a is JObj p && b is JObj q)
        {
            if (p.Count != q.Count) return false;
            foreach (var kv in p) if (!q.TryGetValue(kv.Key, out var w) || !Same(kv.Value, w)) return false;
            return true;
        }
        return false;
    }
}
