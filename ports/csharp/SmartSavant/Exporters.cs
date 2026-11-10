using System.Globalization;
using System.Text;

namespace SmartSavant;

/// <summary>
/// Exporters: a council result to user stories, a spec in Markdown, a fine-tune trainset and a Mermaid tree.
/// Every exporter reports what the run did not decide instead of covering it up.
/// </summary>
public static class Exporters
{
    /// <summary>sha256 of the canonical JSON of the whole result: the run's identity in exports.</summary>
    public static string RunDigest(JObj res) => Canonical.Sha256Hex(Canonical.Encode(res));

    /// <summary>The last iteration of each phase, in phase order.</summary>
    internal static List<JObj> LastRounds(JObj res)
    {
        var order = new List<string>();
        var last = new Dictionary<string, JObj>(StringComparer.Ordinal);
        foreach (var r in Py.L(res, "rounds"))
        {
            string id = Py.Str(Py.M(r)["step_id"]);
            if (!last.ContainsKey(id)) order.Add(id);
            last[id] = Py.M(r);
        }
        return order.Select(k => last[k]).ToList();
    }

    private static JObj Juror(object? r) => Py.M(Py.M(r)["juror"]);

    public static JObj ToStories(JObj res, string projectKey)
    {
        var stories = new List<object?>();
        long ready = 0;
        int n = 0;
        foreach (var r in LastRounds(res))
        {
            var j = Juror(r);
            string sid = $"STORY-{projectKey}-{(++n).ToString("D4", CultureInfo.InvariantCulture)}";
            var steps = Py.L(j, "next_steps").Where(s => !Gates.IsGeneric(s)).ToList();
            if (Equals(j["synthesis_status"], "full") && steps.Count > 0)
            {
                var ac = new List<object?> { "Implements the decision: " + Py.Str(j["decision"]) };
                ac.AddRange(Py.L(j, "resolved").Select(x => (object?)("Holds: " + Py.Str(x))));
                ac.Add("A test exercises it end to end.");
                stories.Add(Py.O("story_id", sid, "status", "ready", "epic", r["step_title"], "title", steps[0], "decision", j["decision"],
                    "sub_tasks", steps, "acceptance_criteria", ac, "open_issues", Py.L(j, "open_issues").ToList(),
                    "phase", r["step_id"], "iteration", r["iteration"]));
                ready++;
            }
            else
            {
                var issues = Py.L(j, "open_issues").ToList();
                if (Py.Truthy(j["error"])) issues.Add(j["error"]);
                stories.Add(Py.O("story_id", sid, "status", "blocked", "epic", r["step_title"],
                    "title", $"Decide {Py.Str(r["step_title"])}: the council did not reach a decision ({Py.Str(j["synthesis_status"])})",
                    "decision", "", "sub_tasks", Py.A(), "acceptance_criteria", Py.A(), "open_issues", issues,
                    "phase", r["step_id"], "iteration", r["iteration"]));
            }
        }
        return Py.O("schema", "smartsavant.stories/1", "project_key", projectKey, "run", RunDigest(res),
            "counts", Py.O("stories", (long)stories.Count, "ready", ready, "blocked", stories.Count - ready), "stories", stories);
    }

    private const string MdMeta = "\\`*_{}[]()#+-.!|~";

    /// <summary>Control (U+0000-U+001F, U+007F-U+009F) and bidirectional-control characters, shown as U+FFFD.</summary>
    internal static bool Bad(char c) => c < 0x20 || (c >= 0x7f && c <= 0x9f) || (c >= 0x202a && c <= 0x202e) || (c >= 0x2066 && c <= 0x2069);

    /// <summary>Model text made inert in Markdown: one line (whitespace squashed), control and bidirectional-control
    /// characters as U+FFFD, &amp; &lt; &gt; as HTML entities, and every Markdown metacharacter
    /// \ ` * _ { } [ ] ( ) # + - . ! | ~ backslash-escaped.</summary>
    public static string Md(object? text)
    {
        var b = new StringBuilder();
        foreach (char c in Gates.Squash(Py.Str(text)))
        {
            if (Bad(c)) b.Append('\ufffd');
            else if (c == '&') b.Append("&amp;");
            else if (c == '<') b.Append("&lt;");
            else if (c == '>') b.Append("&gt;");
            else if (MdMeta.IndexOf(c) >= 0) b.Append('\\').Append(c);
            else b.Append(c);
        }
        return b.ToString();
    }

    public static string ToSpecMd(JObj res)
    {
        var h = Council.Health(res);
        var lines = new List<string> { "# Council spec", "", $"*{Md(res["idea"])}*", "",
            $"Run {RunDigest(res).Substring(0, 16)} with adapter {Md(res["adapter"])}"
            + (Equals(res["synthetic"], true) ? " (synthetic: not a real deliberation)" : "") + ". "
            + $"{Py.Str(h["phases_decided"])} of {Py.Str(h["phases"])} phases decided; views {Py.Repr(h["views"])}; "
            + $"merges {Py.Repr(h["merges"])}; {Py.Str(h["flags"])} flag(s).", "" };
        if (!Equals(h["decided"], true))
        {
            lines.Add("> **Not build-ready:** at least one phase has no full synthesis. Do not build from the "
                + "undecided phases; re-run them with a working adapter.");
            lines.Add("");
        }
        var locked = Py.L(res, "locked_decisions");
        if (locked.Count > 0)
        {
            lines.AddRange(new[] { "## Decisions", "", "| # | Decision |", "|---|---|" });
            for (int i = 0; i < locked.Count; i++) lines.Add($"| {i + 1} | {Md(locked[i])} |");
            lines.Add("");
        }
        foreach (var r in LastRounds(res))
        {
            var j = Juror(r);
            lines.AddRange(new[] { "## " + Md(r["step_title"]), "", "**Question:** " + Md(r["question"]), "" });
            if (Equals(j["synthesis_status"], "full"))
            {
                lines.AddRange(new[] { "**Decision:** " + Md(j["decision"]), "", Md(j["merge_summary"]), "" });
                if (Py.L(j, "next_steps").Count > 0)
                {
                    lines.AddRange(new[] { "**Next steps:**", "" });
                    lines.AddRange(Py.L(j, "next_steps").Select(s => "- " + Md(s)));
                    lines.Add("");
                }
            }
            else
            {
                lines.Add($"**NOT DECIDED** ({Py.Str(j["synthesis_status"])})" + (Py.Truthy(j["error"]) ? ": " + Md(j["error"]) : ""));
                lines.Add("");
            }
            if (Py.L(j, "open_issues").Count > 0)
            {
                lines.AddRange(new[] { "**Open issues:**", "" });
                lines.AddRange(Py.L(j, "open_issues").Select(s => "- " + Md(s)));
                lines.Add("");
            }
            var failed = Py.L(r, "views").Where(v => !Equals(Py.Get(v, "status"), "ok")).Select(v => Py.Str(Py.Get(v, "stakeholder"))).ToList();
            if (failed.Count > 0)
            {
                lines.Add($"*No valid view from: {Md(string.Join(", ", failed))}.*");
                lines.Add("");
            }
        }
        var flags = Py.L(res, "flags");
        if (flags.Count > 0)
        {
            lines.AddRange(new[] { "## Flags", "", $"{flags.Count} finding(s) to review before use:", "" });
            foreach (var f in flags.Take(200))
            {
                object? fig = Py.Get(f, "figure"), src = Py.Get(f, "source");
                object? what = Py.Truthy(fig) ? fig : Py.Truthy(src) ? src : "";
                lines.Add($"- {Py.Str(Py.Get(f, "flag"))}: {Md(what)} ({Md(Py.Get(f, "stakeholder", ""))}, {Md(Py.Get(f, "step_id", ""))})");
            }
            lines.Add("");
        }
        return string.Join("\n", lines);
    }

    public static readonly IReadOnlyDictionary<string, string> Methodology = new Dictionary<string, string>(StringComparer.Ordinal)
    {
        ["frame"] = "Establish the problem, the real users and their jobs-to-be-done, and agree on one long-term goal.",
        ["concept"] = "Collect solution directions, then pick the wedge: the smallest offer the users would switch for.",
        ["design"] = "Draw the user flow and the data flow end to end, and name every screen and every store.",
        ["build"] = "Decide the technical approach and the riskiest part to prove first.",
        ["integrate"] = "Fit the design into the target architecture: owners, interfaces, auth, deployment.",
        ["measure"] = "Set the KPIs, how each one is measured, and the thresholds that would change the plan.",
        ["spec"] = "Assemble the decisions into one document that an engineer can build from.",
    };

    private static readonly string[] ViewFields = { "w5h1", "done_so_far", "missing", "devils_advocate", "contribution", "sources" };

    private static string FlagKey(object? step, object? it, object? who) =>
        Py.JsonDumps(Py.A(step, Py.IsInt(it) ? Py.BigInt(it!) : it, who), false);

    private static JObj With(JObj meta, params object?[] kv)
    {
        var m = new JObj();
        foreach (var e in meta) m[e.Key] = e.Value;
        for (int k = 0; k < kv.Length; k += 2) m[(string)kv[k]!] = kv[k + 1];
        return m;
    }

    /// <summary>Rows {instruction, input, output, meta}: methodology per phase used; expert rows (ok views without a
    /// flag) and facilitation rows (full merges) unless the run is synthetic and allowSynthetic is false.</summary>
    public static List<JObj> ToTrainset(JObj res, bool allowSynthetic = false)
    {
        string digest = RunDigest(res);
        var rows = new List<JObj>();
        var flagged = new HashSet<string>(Py.L(res, "flags").Select(f => FlagKey(Py.Get(f, "step_id"), Py.Get(f, "iteration"), Py.Get(f, "stakeholder"))),
            StringComparer.Ordinal);
        bool learn = allowSynthetic || !Equals(res["synthetic"], true);
        var seen = new HashSet<string>(StringComparer.Ordinal);
        foreach (var ro in Py.L(res, "rounds"))
        {
            var r = Py.M(ro);
            string stepId = Py.Str(r["step_id"]);
            string title = Py.Str(r["step_title"]);
            var meta = Py.O("phase", r["step_id"], "iteration", r["iteration"], "run", digest, "adapter", res["adapter"]);
            if (!seen.Contains(stepId) && Methodology.TryGetValue(stepId, out var text) && Py.Lower(title).StartsWith(stepId, StringComparison.Ordinal))
            {
                seen.Add(stepId);
                rows.Add(Py.O("instruction", $"We are at the {title} phase of a design council. How should I facilitate it?",
                    "input", "", "output", text, "meta", With(meta, "kind", "methodology")));
            }
            if (!learn) continue;
            foreach (var vo in Py.L(r, "views"))
            {
                var v = Py.M(vo);
                if (Equals(v["status"], "ok") && !flagged.Contains(FlagKey(r["step_id"], r["iteration"], v["stakeholder"])))
                {
                    var output = new JObj();
                    foreach (var k in ViewFields) output[k] = v[k];
                    rows.Add(Py.O("instruction", $"As the {Py.Str(v["stakeholder"])} on a design council, give your view on: {Py.Str(r["question"])}",
                        "input", res["idea"], "output", Py.JsonDumps(output, true),
                        "meta", With(meta, "kind", "expert", "stakeholder", v["stakeholder"])));
                }
            }
            var j = Juror(r);
            if (Equals(j["synthesis_status"], "full"))
            {
                var contributions = Py.L(r, "views").Where(v => Equals(Py.Get(v, "status"), "ok")).Select(v => Py.Get(v, "contribution")).ToList();
                rows.Add(Py.O("instruction", $"Merge the panel for the {title} phase as the juror.",
                    "input", Py.JsonDumps(contributions, false),
                    "output", Py.JsonDumps(Py.O("decision", j["decision"], "merge_summary", j["merge_summary"], "resolved", j["resolved"],
                        "next_steps", j["next_steps"]), true),
                    "meta", With(meta, "kind", "facilitation", "synthesis", "full")));
            }
        }
        return rows;
    }

    /// <summary>A Mermaid label: squashed, control characters as U+FFFD, at most 80 code points, quotes as
    /// apostrophes, &amp; &lt; &gt; as entities, quoted.</summary>
    internal static string Label(object? t)
    {
        var b = new StringBuilder();
        foreach (char c in Gates.Squash(Py.Str(t))) b.Append(Bad(c) ? '\ufffd' : c);
        string s = Py.Head(b.ToString(), 80);
        return "\"" + s.Replace("&", "&amp;").Replace("\"", "'").Replace("<", "&lt;").Replace(">", "&gt;") + "\"";
    }

    public static string ToMermaid(JObj res)
    {
        var lines = new List<string> { "flowchart TD", "  start([Idea])" };
        string prev = "start";
        var tree = Py.L(res, "decision_tree");
        var rounds = Py.L(res, "rounds");
        for (int i = 0; i < tree.Count; i++)
        {
            string nid = $"n{i}";
            var n = tree[i];
            var r = Py.M(rounds[i]);
            var j = Juror(r);
            object? label = Equals(j["synthesis_status"], "full") ? Py.Get(n, "decision") : "NOT DECIDED: " + Py.Str(j["synthesis_status"]);
            lines.Add($"  {nid}{{{Label(Py.Str(r["step_title"]) + " #" + Py.Str(Py.Get(n, "iteration")))}}}");
            lines.Add($"  {prev} --> {nid}");
            lines.Add($"  {nid}d[{Label(label)}]");
            lines.Add($"  {nid} --> {nid}d");
            prev = nid;
        }
        return string.Join("\n", lines) + "\n";
    }
}
