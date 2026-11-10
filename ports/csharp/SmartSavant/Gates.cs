using System.Text;
using System.Text.RegularExpressions;

namespace SmartSavant;

/// <summary>
/// Quality gates: pure functions over plain JSON values (vectors: spec/conformance/gates.json, figures.json).
/// .NET's <c>\s</c>, <c>\w</c>, <c>\d</c> and <c>\b</c> are Unicode-aware and RegexOptions.IgnoreCase folds
/// some non-ASCII letters (U+212A KELVIN SIGN matches k), so every pattern here spells out the ASCII classes
/// of SEMANTICS.md section 2 and is made case-insensitive by <see cref="Ci"/>, which turns each ASCII letter
/// into a two-letter class.
/// </summary>
public static class Gates
{
    private const string S = "[ \\t\\n\\r\\f\\v]";
    private const string W = "[A-Za-z0-9_]";
    private const string NW = "[^A-Za-z0-9_]";
    private const string D = "[0-9]";
    private const string B = "(?:(?<=" + W + ")(?!" + W + ")|(?<!" + W + ")(?=" + W + "))";

    /// <summary>A pattern made ASCII case-insensitive: letters outside character classes and escapes become [xX].</summary>
    internal static string Ci(string pattern)
    {
        var b = new StringBuilder();
        bool inClass = false;
        for (int k = 0; k < pattern.Length; k++)
        {
            char c = pattern[k];
            if (c == '\\' && k + 1 < pattern.Length) { b.Append(c).Append(pattern[++k]); continue; }
            if (inClass) { if (c == ']') inClass = false; b.Append(c); continue; }
            if (c == '[') { inClass = true; b.Append(c); if (k + 1 < pattern.Length && pattern[k + 1] == '^') b.Append(pattern[++k]); if (k + 1 < pattern.Length && pattern[k + 1] == ']') b.Append(pattern[++k]); continue; }
            if (c == '(' && k + 2 < pattern.Length && pattern[k + 1] == '?')
            {
                // group constructs: copy "(?:", "(?=", "(?!", "(?<=", "(?<!" as they are
                b.Append("(?");
                k++;
                while (k + 1 < pattern.Length && "<:=!".IndexOf(pattern[k + 1]) >= 0) b.Append(pattern[++k]);
                continue;
            }
            if (c < 128 && char.IsLetter(c)) b.Append('[').Append(char.ToLowerInvariant(c)).Append(char.ToUpperInvariant(c)).Append(']');
            else b.Append(c);
        }
        return b.ToString();
    }

    private const string Unit = "%|percent|ms|\u00b5s|us|ns|s|sec|secs|seconds?|min|mins|minutes?|h|hrs?|hours?|days?|weeks?|"
        + "months?|years?|x|k|K|m|M|b|B|bn|mn|GB|MB|KB|TB|GiB|MiB|KiB|TiB|kpps|pps|rps|qps|tps|req/s|tokens?|users?|agents?|"
        + "USD|EUR|GBP";

    private static readonly Regex Fig = new(
        "(?<![A-Za-z0-9_.])"
        + "(?:(?<cur>[\u00a3$\u20ac])" + S + "?)?"
        + "(?<num>" + D + "{1,3}(?:," + D + "{3})+(?:\\." + D + "+)?|" + D + "+\\." + D + "+|" + D + "+)"
        + "(?:" + S + "?(?<unit>" + Unit + "))?"
        + "(?!" + W + ")");

    private static bool Ws(char c) => c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == '\v';

    /// <summary>Runs of ASCII whitespace (space, tab, LF, CR, FF, VT) to one space, trimmed.</summary>
    public static string Squash(string? text)
    {
        if (string.IsNullOrEmpty(text)) return "";
        var b = new StringBuilder();
        int k = 0;
        while (k < text.Length)
        {
            while (k < text.Length && Ws(text[k])) k++;
            int st = k;
            while (k < text.Length && !Ws(text[k])) k++;
            if (k > st)
            {
                if (b.Length > 0) b.Append(' ');
                b.Append(text, st, k - st);
            }
        }
        return b.ToString();
    }

    /// <summary>True for a non-string, or a string that is empty or made only of [ \t\n\r\f\v].</summary>
    public static bool Blank(object? text) => text is not string s || s.All(Ws);

    /// <summary><c>text or ''</c> for the text gates: a string as is, a falsy value as "", anything else as str().</summary>
    private static string Text(object? v) => v is string s ? s : Py.Truthy(v) ? Py.Str(v) : "";

    /// <summary>Figures in order of appearance, as written.</summary>
    public static List<string> Figures(object? text)
    {
        var output = new List<string>();
        foreach (Match m in Fig.Matches(Text(text)))
        {
            string num = m.Groups["num"].Value;
            if (m.Groups["cur"].Success || m.Groups["unit"].Success || num.Contains(',') || num.Contains('.'))
                output.Add(m.Value.Trim());
        }
        return output;
    }

    private static string ViewText(object? view)
    {
        object? missing = Py.Get(view, "missing", "");
        if (missing is List<object?> l) missing = string.Join(" ", l.Select(Py.Str));
        return string.Join(" ", Py.Str(Py.Get(view, "contribution", "")), Py.Str(Py.Get(view, "devils_advocate", "")),
            Py.Str(Py.Get(view, "done_so_far", "")), Py.Str(missing));
    }

    /// <summary>
    /// Flags for one view: unsourced_figures for every figure when it cites no source, then unknown_source per
    /// cited id not in allowedSources (when the run allows no sources, every cited id is unknown).
    /// </summary>
    public static List<JObj> FlagUnsourced(object? view, object? allowedSources)
    {
        var output = new List<JObj>();
        object? who = Py.Get(view, "stakeholder", "");
        object? src = Py.Get(view, "sources", new List<object?>());
        var cited = Py.Truthy(src) ? Py.Iter(src).OfType<string>().ToList() : new List<string>();
        if (cited.Count == 0)
        {
            foreach (var f in Figures(ViewText(view))) output.Add(Py.O("flag", "unsourced_figures", "figure", f, "stakeholder", who));
        }
        var allowed = new HashSet<string>(Py.Truthy(allowedSources) ? Py.Iter(allowedSources).OfType<string>() : Array.Empty<string>(), StringComparer.Ordinal);
        foreach (var s in cited)
        {
            if (!allowed.Contains(s)) output.Add(Py.O("flag", "unknown_source", "source", s, "stakeholder", who));
        }
        return output;
    }

    private static readonly Regex[] Generic =
    {
        new(Ci("^" + S + "*advance the [A-Za-z0-9_/ -]+ with the decided approach[.!?]*" + S + "*$")),
        new("^" + S + "*[A-Za-z]+" + S + "+[XYZ][.!?]*" + S + "*$"),
        new(Ci("^" + S + "*(next steps?" + S + "*:?" + S + "*)?(tbd|todo|n/?a|none|proceed|continue|next steps?)?[.!?]*" + S + "*$")),
    };

    private static readonly Regex Fallback = new(Ci("synthesis failed|(falling|fell|fall) back to the raw panel|"
        + "returned unparseable output|unparseable output|offline fallback|\\(offline\\)|fallback template"));

    public static bool IsFallback(object? text) => Fallback.IsMatch(Text(text));

    /// <summary>A next step or story title that names no real component or decision (fallback text included).</summary>
    public static bool IsGeneric(object? step) => Blank(step) || Generic.Any(p => p.IsMatch((string)step!)) || IsFallback(step);

    private static readonly Regex Placeholder = new(Ci("#" + S + "*TODO" + B + "|//" + S + "*TODO" + B
        + "|return" + S + "*\\{" + S + "*['\"]ok['\"]" + S + "*:" + S + "*True" + S + "*\\}|raise NotImplementedError|"
        + B + "pass" + S + "*#" + S + "*TODO"));

    public static bool HasPlaceholderCode(object? text) => Placeholder.IsMatch(Text(text));

    /// <summary>A merge that genuinely decided something.</summary>
    public static bool SynthesisOk(object? merge)
    {
        if (!Equals(Py.Get(merge, "synthesis_status", "full"), "full")) return false;
        object? summary = Py.Get(merge, "merge_summary", "");
        object? decision = Py.Get(merge, "decision", "");
        if (Blank(summary) || Blank(decision)) return false;
        object? resolved = Py.Get(merge, "resolved");
        object? steps = Py.Get(merge, "next_steps");
        var rs = Py.Truthy(resolved) ? Py.Iter(resolved) : new List<object?>();
        var ss = Py.Truthy(steps) ? Py.Iter(steps) : new List<object?>();
        if (new[] { summary, decision }.Concat(rs).Concat(ss).Any(IsFallback)) return false;
        return Py.Truthy(resolved) && ss.Any(s => !IsGeneric(s));
    }

    /// <summary>A compliance rule: it fires when the lowercased text contains a word from every group.</summary>
    public sealed record Rule(string Name, IReadOnlyList<IReadOnlyList<string>> Groups);

    public static readonly IReadOnlyList<Rule> DefaultRules = new[]
    {
        new Rule("possible trading guidance on non-public information", new[]
        {
            new[] { "non-public", "nonpublic", "mnpi", "insider information" },
            new[] { "buy", "sell", "trade", "short", "position" },
        }),
        new Rule("possible credential or secret in the output", new[]
        {
            new[] { "-----begin", "aws_secret_access_key", "api_key=", "password=", "ghp_", "github" + "_pat_", "pypi-ag" },
        }),
    };

    /// <summary>Reasons to block a release of this text (empty when no rule fired). Keyword matching only.</summary>
    public static List<string> ComplianceBlock(object? text, IReadOnlyList<Rule>? rules = null)
    {
        string low = Py.Lower(Text(text));
        return (rules ?? DefaultRules)
            .Where(r => r.Groups.All(g => g.Any(w => low.Contains(w, StringComparison.Ordinal))))
            .Select(r => r.Name).ToList();
    }

    private static readonly Regex Secret = new(B + Ci("(api[_-]?key|access[_-]?token|token|secret|password|passwd|signature|sig|key)")
        + "(" + S + "*[=:]" + S + "*)([^ \\t\\n\\r\\f\\v&;,\"']+)");
    private static readonly Regex Bearer = new(B + Ci("(bearer|basic)") + S + "+[A-Za-z0-9._~+/=-]{8,}");

    /// <summary>Error text made safe to record: key=VALUE-style secrets and bearer tokens replaced by ***.</summary>
    public static string Redact(string? text)
    {
        string t = Secret.Replace(text ?? "", m => m.Groups[1].Value + m.Groups[2].Value + "***");
        return Bearer.Replace(t, m => m.Groups[1].Value + " ***");
    }
}
