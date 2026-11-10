using System.Globalization;
using System.Numerics;

namespace SmartSavant;

/// <summary>
/// Reads council exports written by other tools (the SmartSignal council) into a council result, so that the
/// gates, the audit and the exporters work on them. Two shapes are recognised: a council result (rounds with
/// views that hold the fields) and a rounds export (views with a "parsed" object). A view whose source is
/// "offline" becomes status offline; a full merge that fails SynthesisOk becomes thin; object contributions
/// are flattened to "key: value" lines; a repeated (step_id, iteration) pair gets ".2", ".3", ... appended.
/// Type-robust: values of the wrong type are skipped or read as empty.
/// </summary>
public static class Importer
{
    private static readonly HashSet<string> Status = new(StringComparer.Ordinal) { "full", "recovered", "offline", "failed", "thin", "no_views" };
    public const int MaxDepth = 20;

    /// <summary>Unpaired surrogates (which cannot be written as UTF-8) become U+FFFD.</summary>
    internal static string Clean(string t) => Py.ReplaceLone(t, "\ufffd");

    /// <summary>A value as text: strings as is, lists joined with "; ", objects as "key: value" lines, deeper than 20 levels "[nested too deep]".</summary>
    public static string Text(object? v) => Text(v, 0);

    private static string Text(object? v, int depth)
    {
        if (depth > MaxDepth) return "[nested too deep]";
        switch (v)
        {
            case null: return "";
            case string s: return Clean(s);
            case JObj o: return string.Join("\n", o.Select(kv => Text(kv.Key, depth + 1) + ": " + Text(kv.Value, depth + 1)));
            case List<object?> l: return string.Join("; ", l.Select(x => Text(x, depth + 1)));
            case bool b: return b ? "true" : "false";
            case double or float:
                double d = Py.ToDouble(v);
                return double.IsNaN(d) || double.IsInfinity(d) ? "float" : Py.Num(d);
        }
        if (Py.IsInt(v)) return Py.BigInt(v).ToString(CultureInfo.InvariantCulture);
        return v.GetType().Name;
    }

    private static JObj Dict(object? v) => v as JObj ?? new JObj();

    private static List<object?> List(object? v) => v as List<object?> ?? new List<object?>();

    private static List<object?> Strs(object? v)
    {
        switch (v)
        {
            case null: return new List<object?>();
            case string s: return Gates.Blank(s) ? new List<object?>() : new List<object?> { s };
            case List<object?> l: return l.Select(x => Text(x)).Where(t => !Gates.Blank(t)).Cast<object?>().ToList();
            default: return new List<object?> { Text(v) };
        }
    }

    /// <summary>The idea as text: a string as is; {name, problem, solution, audience} as one line.</summary>
    public static string IdeaText(object? idea)
    {
        if (idea is string s) return s;
        if (idea is JObj)
        {
            string name = Text(Py.Get(idea, "name", "")), prob = Text(Py.Get(idea, "problem", ""));
            string sol = Text(Py.Get(idea, "solution", "")), aud = Text(Py.Get(idea, "audience", ""));
            string t = Py.Strip($"{name}: {prob}", ": ");
            if (sol.Length > 0) t += $" -> {sol}";
            if (aud.Length > 0) t += $" (audience: {aud})";
            return t;
        }
        return Text(idea);
    }

    /// <summary>"smartsavant", "smartsignal-rounds", "smartsignal-result" or null.</summary>
    public static string? Detect(object? doc)
    {
        if (doc is not JObj) return null;
        if (Equals(Py.Get(doc, "schema"), "smartsavant.council/1")) return "smartsavant";
        if (Py.Get(doc, "rounds") is List<object?> rounds && rounds.Count > 0 && rounds.Any(r => r is JObj o && o.ContainsKey("views")))
        {
            foreach (var r in rounds)
            {
                foreach (var v in List(Py.Get(Dict(r), "views")))
                {
                    if (v is JObj vo) return vo.ContainsKey("parsed") ? "smartsignal-rounds" : "smartsignal-result";
                }
            }
            return "smartsignal-result";
        }
        return null;
    }

    /// <summary>Python's <c>type(x) is int and 0 &lt;= x &lt; 1000</c>.</summary>
    private static bool SmallInt(object? x) => Py.IsInt(x) && Py.BigInt(x!) >= 0 && Py.BigInt(x!) < 1000;

    /// <summary>A SmartSignal council export as a council result; ArgumentException when doc is not one.</summary>
    public static JObj FromSmartsignal(object? doc)
    {
        string? kind = Detect(doc);
        if (kind != "smartsignal-rounds" && kind != "smartsignal-result")
            throw new ArgumentException("not a SmartSignal council export (no rounds with views)");
        var models = new SortedSet<string>(Py.Order);
        var rounds = new List<object?>();
        var tree = new List<object?>();
        var flags = new List<object?>();
        var locked = new List<object?>();
        var res = Model.CouncilResult(Py.O("idea", Clean(IdeaText(Py.Get(doc, "idea", ""))), "synthetic", false,
            "rounds", rounds, "decision_tree", tree, "flags", flags, "locked_decisions", locked));
        var seen = new Dictionary<string, int>(StringComparer.Ordinal);
        var names = new List<string>();
        foreach (var ro in Py.L(Py.Get(doc, "rounds")))
        {
            if (ro is not JObj r) continue;
            string step = Text(Py.Get(r, "step_id", "step"));
            if (step.Length == 0) step = "step";
            object? iter = Py.Get(r, "iteration"), rnd = Py.Get(r, "round");
            long it = SmallInt(iter) ? (long)Py.BigInt(iter!) + 1 : SmallInt(rnd) ? (long)Py.BigInt(rnd!) + 1 : 1;
            string key = Py.JsonDumps(Py.A(step, it), false);
            int count = seen.GetValueOrDefault(key) + 1;
            seen[key] = count;
            if (count > 1) step = $"{step}.{count}";
            var views = new List<object?>();
            foreach (var vo in List(Py.Get(r, "views")))
            {
                if (vo is not JObj v) continue;
                var p = Dict(kind == "smartsignal-rounds" ? Py.Get(v, "parsed", v) : v);
                string src = Text(Py.Get(v, "source", ""));
                if (Py.Truthy(Py.Get(v, "model"))) models.Add(Text(v["model"]));
                var w = Dict(Py.Get(p, "w5h1"));
                string who = Text(Py.Get(v, "stakeholder", ""));
                if (!names.Contains(who)) names.Add(who);
                var w5 = new JObj();
                foreach (var k in Schema.W5h1Keys) w5[k] = Text(Py.Get(w, k, ""));
                views.Add(Model.StakeholderView(Py.O("stakeholder", who, "role", Text(Py.Get(v, "role", "")),
                    "status", src == "offline" ? "offline" : "ok", "w5h1", w5, "done_so_far", Text(Py.Get(p, "done_so_far")),
                    "missing", Strs(Py.Get(p, "missing")), "devils_advocate", Text(Py.Get(p, "devils_advocate")),
                    "contribution", Text(Py.Get(p, "contribution")),
                    "sources", Strs(Py.Get(p, "sources")).Where(s => Py.Len((string)s!) <= 200).ToList())));
            }
            var j = Dict(Py.Get(r, "juror"));
            if (Py.Truthy(Py.Get(j, "model"))) models.Add(Text(j["model"]));
            string st = Text(Py.Get(j, "synthesis_status", ""));
            if (st.Length == 0 || !Status.Contains(st)) st = "failed";
            var resolved = Strs(Py.Get(j, "resolved"));
            string decision = Text(Py.Get(j, "decision"));
            if (decision.Length == 0) decision = resolved.Count > 0 ? (string)resolved[0]! : "";
            var juror = Model.JurorMerge(Py.O("synthesis_status", st, "merge_summary", Text(Py.Get(j, "merge_summary")),
                "decision", decision, "resolved", resolved, "open_issues", Strs(Py.Get(j, "open_issues")),
                "next_steps", Strs(Py.Get(j, "next_steps")), "reversals", Strs(Py.Get(j, "reversals")),
                "spec_delta", Text(Py.Get(j, "spec_delta")), "loop_recommended", Equals(Py.Get(j, "loop_recommended"), true)));
            if (st == "full" && !Gates.SynthesisOk(juror))
            {
                juror["synthesis_status"] = "thin";
                juror["error"] = "imported as full, but it fails the synthesis gate";
            }
            string title = Text(Py.Get(r, "step_title", step));
            rounds.Add(Model.CouncilRound(step, title.Length > 0 ? title : step, it,
                Text(Py.Get(r, "question", Py.Get(r, "step_title", ""))), views, juror));
            tree.Add(Model.DecisionNode(step, it, views.Where(x => Equals(Py.Get(x, "status"), "ok")).Select(x => Py.Get(x, "contribution")).ToList(),
                juror["decision"], Py.L(juror, "next_steps").ToList()));
            foreach (var xo in views)
            {
                var x = Py.M(xo);
                if (!Equals(x["status"], "ok")) continue;
                var fv = Py.O("stakeholder", x["stakeholder"], "contribution", x["contribution"], "devils_advocate", x["devils_advocate"],
                    "done_so_far", x["done_so_far"], "missing", x["missing"], "sources", x["sources"]);
                foreach (var fl in Gates.FlagUnsourced(fv, Py.A()))
                {
                    fl["step_id"] = step;
                    fl["iteration"] = it;
                    flags.Add(fl);
                }
            }
            if (Equals(juror["synthesis_status"], "full"))
            {
                foreach (var d in Py.L(juror, "resolved")) if (!locked.Any(y => Equals(y, d))) locked.Add(d);
            }
        }
        bool anyFull = rounds.Any(r => Equals(Py.Get(Py.Get(r, "juror"), "synthesis_status"), "full"));
        if (anyFull && Py.Get(doc, "decisions") is List<object?> decs && decs.Count > 0 && decs.All(d => d is JObj))
        {
            res["locked_decisions"] = decs.Select(d => $"{Text(Py.Get(d, "topic", ""))}: {Text(Py.Get(d, "decision", ""))}")
                .Where(x => !Gates.IsFallback(x)).Cast<object?>().ToList();
        }
        var sh = List(Py.Get(doc, "stakeholders")).Select(s => (object?)(s is JObj ? Text(Py.Get(s, "name", "")) : Text(s))).ToList();
        res["stakeholders"] = sh.Count > 0 ? sh : names.Cast<object?>().ToList();
        res["adapter"] = models.Count > 0 ? "smartsignal:" + string.Join(",", models) : "smartsignal";
        return res;
    }
}
