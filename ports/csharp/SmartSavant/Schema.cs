using System.Collections;
using System.Numerics;
using System.Text;
using System.Text.RegularExpressions;

namespace SmartSavant;

/// <summary>An answer that is not plain JSON data (schema.NotPlainData in Python).</summary>
public sealed class NotPlainData : ArgumentException
{
    public NotPlainData(string message) : base(message) { }
}

/// <summary>
/// Validation of what an adapter returns, and of council files. The error strings and their order are part
/// of the contract (spec/conformance/schema.json).
/// </summary>
public static class Schema
{
    public const int MaxStr = 8000;
    public const int MaxDelta = 20000;
    public const int MaxItems = 20;
    public const int MaxItem = 2000;
    public const int MaxDepth = 32;
    /// <summary>Code points of all strings in one answer, keys included.</summary>
    public const int MaxTotal = 1_000_000;
    public static readonly Regex SourceId = new("^[A-Za-z0-9._:/#@-]{1,200}\\z");
    public static readonly IReadOnlyList<string> W5h1Keys = new[] { "who", "what", "where", "when", "why", "how" };
    public static readonly IReadOnlyList<string> ViewKeys = new[] { "w5h1", "done_so_far", "missing", "devils_advocate", "contribution", "sources" };
    public static readonly IReadOnlyList<string> MergeKeys = new[]
    {
        "merge_summary", "decision", "resolved", "open_issues", "next_steps", "reversals", "spec_delta", "loop_recommended",
    };
    private static readonly IReadOnlyList<string> MergeRequired = MergeKeys.Where(k => k != "reversals").ToArray();

    /// <summary>
    /// A copy of an adapter answer made only of null, bool, long (or BigInteger), double, string,
    /// <c>List&lt;object?&gt;</c> and <see cref="JObj"/>, built before anything else so that what is validated is
    /// exactly what is used. Smaller integer types become long, float becomes double, any IList becomes a list
    /// and any IDictionary with string keys an object. Other types, keys that are not strings, nesting deeper
    /// than 32 and more than 1,000,000 code points of text throw <see cref="NotPlainData"/>.
    /// </summary>
    public static object? Plain(object? obj)
    {
        int budget = MaxTotal;
        return Go(obj, 0, ref budget);
    }

    private static object? Go(object? x, int depth, ref int budget)
    {
        if (depth > MaxDepth) throw new NotPlainData("nested deeper than 32 levels");
        switch (x)
        {
            case null: return null;
            case bool or long or double or BigInteger: return x;
            case float f: return (double)f;
            case string s:
                budget -= Py.Len(s);
                if (budget < 0) throw new NotPlainData("more than 1,000,000 characters of text");
                return s;
            case JObj o:
            {
                var output = new JObj();
                foreach (var kv in o.ToList())
                {
                    budget -= Py.Len(kv.Key);
                    output[kv.Key] = Go(kv.Value, depth + 1, ref budget);
                }
                if (budget < 0) throw new NotPlainData("more than 1,000,000 characters of text");
                return output;
            }
            case IDictionary d:
            {
                var output = new JObj();
                var entries = new List<DictionaryEntry>();
                var en = d.GetEnumerator(); // IDictionaryEnumerator: DictionaryEntry items for any IDictionary
                while (en.MoveNext()) entries.Add(en.Entry);
                foreach (var e in entries)
                {
                    if (e.Key is not string k) throw new NotPlainData("object keys must be strings");
                    budget -= Py.Len(k);
                    output[k] = Go(e.Value, depth + 1, ref budget);
                }
                if (budget < 0) throw new NotPlainData("more than 1,000,000 characters of text");
                return output;
            }
            case IList l:
            {
                var output = new List<object?>(l.Count);
                foreach (var v in l.Cast<object?>().ToList()) output.Add(Go(v, depth + 1, ref budget));
                return output;
            }
        }
        if (Py.IsInt(x)) return x is BigInteger ? x : (object)(long)Py.BigInt(x);
        throw new NotPlainData($"{x.GetType().Name} is not JSON data");
    }

    /// <summary>An unknown key as shown in an error: control characters dropped, unpaired
    /// surrogates as U+FFFD, then at most 40 code points.</summary>
    private static string ShowKey(string k)
    {
        var b = new StringBuilder();
        foreach (var cp in Py.CodePoints(k))
        {
            int c = cp[0];
            if (c < 32 || (c >= 127 && c < 160)) continue;
            // per code point, so that two lone surrogates around a dropped character never become a pair;
            // error text is recorded, so it must stay encodable
            b.Append(cp.Length == 1 && char.IsSurrogate(cp[0]) ? "\ufffd" : cp);
        }
        string s = b.ToString();
        return Py.Head(s, 40) + (Py.Len(s) > 40 ? "..." : "");
    }

    private static void CheckStr(List<string> errs, string where, object? v, int limit = MaxStr, bool nonempty = false)
    {
        if (v is not string s) errs.Add($"{where} must be a string");
        else if (Py.Len(s) > limit) errs.Add($"{where} is longer than {limit} characters");
        else if (s.Contains('\0')) errs.Add($"{where} contains a NUL character");
        else if (Py.HasLone(s)) errs.Add($"{where} contains an unpaired surrogate");
        else if (nonempty && Gates.Blank(s)) errs.Add($"{where} must not be empty");
    }

    private static void CheckList(List<string> errs, string where, object? v, Regex? pattern = null)
    {
        if (v is not List<object?> l)
        {
            errs.Add($"{where} must be a list of strings");
            return;
        }
        if (l.Count > MaxItems) errs.Add($"{where} has more than {MaxItems} items");
        for (int i = 0; i < Math.Min(l.Count, MaxItems + 1); i++)
        {
            object? x = l[i];
            if (Gates.Blank(x)) errs.Add($"{where}[{i}] must be a non-empty string");
            else if (Py.Len((string)x!) > MaxItem) errs.Add($"{where}[{i}] is longer than {MaxItem} characters");
            else if (pattern != null && !pattern.IsMatch((string)x!)) errs.Add($"{where}[{i}] is not a valid source id");
            else if (((string)x!).Contains('\0')) errs.Add($"{where}[{i}] contains a NUL character");
            else if (Py.HasLone((string)x!)) errs.Add($"{where}[{i}] contains an unpaired surrogate");
        }
    }

    /// <summary>Adds the unknown-keys and missing-keys errors; true when keys are missing.</summary>
    private static bool UnknownAndMissing(List<string> errs, JObj o, IReadOnlyList<string> allowed, IReadOnlyList<string> required)
    {
        var extra = Py.Sorted(o.Keys.Where(k => !allowed.Contains(k)));
        if (extra.Count > 0) errs.Add($"unknown keys ({extra.Count}): " + string.Join(", ", extra.Take(5).Select(ShowKey)));
        var missing = Py.Sorted(required.Where(k => !o.ContainsKey(k)));
        if (missing.Count > 0) errs.Add("missing keys: " + string.Join(", ", missing));
        return missing.Count > 0;
    }

    /// <summary>Empty when v is a valid view; otherwise the reasons, first problem first.</summary>
    public static List<string> CheckView(object? v)
    {
        var errs = new List<string>();
        if (v is not JObj o) return new List<string> { "a view must be a JSON object" };
        if (UnknownAndMissing(errs, o, ViewKeys, ViewKeys)) return errs;
        if (o["w5h1"] is not JObj w || w.Count != W5h1Keys.Count || !W5h1Keys.All(w.ContainsKey))
            errs.Add("w5h1 must be an object with exactly who, what, where, when, why, how");
        else
            foreach (var k in W5h1Keys) CheckStr(errs, "w5h1." + k, w[k]);
        CheckStr(errs, "done_so_far", o["done_so_far"]);
        if (o["missing"] is string) CheckStr(errs, "missing", o["missing"]);
        else CheckList(errs, "missing", o["missing"]);
        CheckStr(errs, "devils_advocate", o["devils_advocate"], MaxStr, true);
        CheckStr(errs, "contribution", o["contribution"]);
        CheckList(errs, "sources", o["sources"], SourceId);
        return errs;
    }

    /// <summary>Empty when m is a valid merge; otherwise the reasons, first problem first.</summary>
    public static List<string> CheckMerge(object? m)
    {
        var errs = new List<string>();
        if (m is not JObj o) return new List<string> { "a merge must be a JSON object" };
        if (UnknownAndMissing(errs, o, MergeKeys, MergeRequired)) return errs;
        CheckStr(errs, "merge_summary", o["merge_summary"], MaxStr, true);
        CheckStr(errs, "decision", o["decision"]);
        foreach (var k in new[] { "resolved", "open_issues", "next_steps" }) CheckList(errs, k, o[k]);
        if (o.ContainsKey("reversals")) CheckList(errs, "reversals", o["reversals"]);
        CheckStr(errs, "spec_delta", o["spec_delta"], MaxDelta);
        if (o["loop_recommended"] is not bool) errs.Add("loop_recommended must be true or false");
        return errs;
    }

    /// <summary>A valid view with missing always a list (a shallow copy).</summary>
    public static JObj NormalizeView(JObj v)
    {
        var output = new JObj();
        foreach (var kv in v) output[kv.Key] = kv.Value;
        if (output["missing"] is string s) output["missing"] = Gates.Blank(s) ? new List<object?>() : new List<object?> { s };
        return output;
    }

    // ---------------------------------------------------------------- council files
    private static readonly string[] StatusV = { "ok", "failed", "offline" };
    private static readonly string[] StatusJ = { "full", "thin", "recovered", "failed", "offline", "no_views" };
    private static readonly string[] Top = { "schema", "idea", "adapter", "synthetic", "stakeholders", "sources", "rounds", "decision_tree", "locked_decisions", "flags" };
    private static readonly (string, char)[] ViewRec =
    {
        ("stakeholder", 's'), ("role", 's'), ("status", 's'), ("w5h1", 'w'), ("done_so_far", 's'), ("missing", 'l'),
        ("devils_advocate", 's'), ("contribution", 's'), ("sources", 'l'), ("error", 's'),
    };
    private static readonly (string, char)[] JurorRec =
    {
        ("synthesis_status", 's'), ("merge_summary", 's'), ("decision", 's'), ("resolved", 'l'), ("open_issues", 'l'),
        ("next_steps", 'l'), ("reversals", 'l'), ("spec_delta", 's'), ("loop_recommended", 'b'), ("error", 's'),
    };
    private static readonly (string, char)[] RoundRec =
    {
        ("step_id", 's'), ("step_title", 's'), ("iteration", 'i'), ("question", 's'), ("views", 'L'), ("juror", 'J'),
    };
    private static readonly (string, char)[] NodeRec =
    {
        ("step_id", 's'), ("iteration", 'i'), ("branches", 'l'), ("decision", 's'), ("next_steps", 'l'),
    };

    private static bool GoodStr(object? x) => x is string s && !Py.HasLone(s);
    private static bool GoodList(object? v) => v is List<object?> l && l.All(GoodStr);
    private static bool SameKeys(object? o, IReadOnlyCollection<string> keys) => o is JObj m && m.Count == keys.Count && keys.All(m.ContainsKey);

    private static bool TypeOk(char t, object? v) => t switch
    {
        's' => GoodStr(v),
        'i' => Py.IsInt(v) && Py.BigInt(v!) >= 1,
        'b' => v is bool,
        'l' => GoodList(v),
        'w' => SameKeys(v, W5h1Keys.ToArray()) && ((JObj)v!).Values.All(GoodStr),
        'L' => v is List<object?>,
        _ => v is JObj,
    };

    private static void Typed(List<string> errs, string where, object? d, (string, char)[] spec)
    {
        var keys = spec.Select(s => s.Item1).ToArray();
        if (!SameKeys(d, keys))
        {
            errs.Add($"{where} must have exactly the keys " + string.Join(", ", keys));
            return;
        }
        foreach (var (k, t) in spec) if (!TypeOk(t, ((JObj)d!)[k])) errs.Add($"{where}.{k} has the wrong type");
    }

    /// <summary>A council.json written by an engine of this package: exact keys and JSON types everywhere, known
    /// statuses, no unpaired surrogates. At most 50 errors.</summary>
    public static List<string> CheckCouncil(object? doc)
    {
        var errs = new List<string>();
        if (doc is not JObj d) return new List<string> { "a council file must be a JSON object" };
        if (!Equals(d["schema"], "smartsavant.council/1")) return new List<string> { "schema must be \"smartsavant.council/1\"" };
        if (!SameKeys(d, Top)) return new List<string> { "top-level keys must be exactly " + string.Join(", ", Py.Sorted(Top)) };
        foreach (var k in new[] { "idea", "adapter" }) if (!GoodStr(d[k])) errs.Add($"{k} must be a string");
        if (d["synthetic"] is not bool) errs.Add("synthetic must be true or false");
        foreach (var k in new[] { "stakeholders", "sources", "locked_decisions" }) if (!GoodList(d[k])) errs.Add($"{k} must be a list of strings");
        if (d["rounds"] is not List<object?> rounds || d["decision_tree"] is not List<object?> tree || d["flags"] is not List<object?> flags)
        {
            errs.Add("rounds, decision_tree and flags must be lists");
            return errs;
        }
        if (tree.Count != rounds.Count) errs.Add("decision_tree must have one node per round");
        for (int i = 0; i < rounds.Count; i++)
        {
            string w = $"rounds[{i}]";
            Typed(errs, w, rounds[i], RoundRec);
            if (rounds[i] is not JObj r || r["views"] is not List<object?> views || r["juror"] is not JObj juror) continue;
            for (int j = 0; j < views.Count; j++)
            {
                Typed(errs, $"{w}.views[{j}]", views[j], ViewRec);
                if (views[j] is JObj v && !(v["status"] is string st && StatusV.Contains(st)))
                    errs.Add($"{w}.views[{j}].status must be one of {string.Join(", ", StatusV)}");
            }
            Typed(errs, $"{w}.juror", juror, JurorRec);
            if (!(juror["synthesis_status"] is string ss && StatusJ.Contains(ss)))
                errs.Add($"{w}.juror.synthesis_status must be one of {string.Join(", ", StatusJ)}");
        }
        for (int i = 0; i < tree.Count; i++) Typed(errs, $"decision_tree[{i}]", tree[i], NodeRec);
        for (int i = 0; i < flags.Count; i++)
        {
            if (flags[i] is not JObj f || !f.Values.All(x => GoodStr(x) || Py.IsInt(x)))
                errs.Add($"flags[{i}] must be an object of strings and integers");
        }
        return errs.Take(50).ToList();
    }
}
