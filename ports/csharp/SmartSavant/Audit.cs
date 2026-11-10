namespace SmartSavant;

/// <summary>
/// Audit a council run (or its stories, or its trainset) before anyone builds or trains from it. The verdict is
/// "build-ready" only when no finding has severity "error". Codes (stable):
/// AUDIT-NO-SYNTHESIS (error), AUDIT-OFFLINE-VIEWS (error, warning when fewer than half), AUDIT-FAILED-VIEWS
/// (warning), AUDIT-FALLBACK-TEXT (error), AUDIT-TEMPLATE-VIEWS (error), AUDIT-GENERIC-STEPS (warning, error when
/// all are), AUDIT-PLACEHOLDER-CODE (warning), AUDIT-UNSOURCED (warning), AUDIT-COMPLIANCE (error),
/// AUDIT-SYNTHETIC (error), AUDIT-TRAINSET-CONTAMINATED (error), AUDIT-TRAINSET-UNREADABLE (error).
/// A council is not taken at its word: a merge recorded as full that fails SynthesisOk counts as not decided,
/// and the adapters stub, scripted and demo* count as synthetic whatever the file says.
/// </summary>
public static class Audit
{
    public const int MaxExamples = 5;
    private static readonly string[] SyntheticAdapters = { "stub", "scripted" };

    private static JObj Finding(string code, string severity, long count, string detail, IEnumerable<object?>? examples = null) =>
        Py.O("code", code, "severity", severity, "count", count, "detail", detail,
            "examples", (examples ?? Enumerable.Empty<object?>()).Take(MaxExamples).Select(e => (object?)Py.Head(Py.Str(e), 200)).ToList());

    private static JObj Juror(object? r) => Py.M(Py.M(r)["juror"]);

    private static bool Decided(object? r) => Equals(Juror(r)["synthesis_status"], "full") && Gates.SynthesisOk(Juror(r));

    public static JObj AuditResult(JObj res)
    {
        var h = Council.Health(res);
        var output = new List<JObj>();
        var last = Exporters.LastRounds(res);
        var rounds = Py.L(res, "rounds");
        var undecided = last.Where(r => !Decided(r)).Select(r =>
        {
            string st = Py.Str(Juror(r)["synthesis_status"]);
            return (object?)($"{Py.Str(r["step_id"])}: {st}" + (st == "full" ? " (recorded, but it fails the synthesis gate)" : ""));
        }).ToList();
        if (undecided.Count > 0)
            output.Add(Finding("AUDIT-NO-SYNTHESIS", "error", undecided.Count, $"{undecided.Count} of {last.Count} phases have no full synthesis", undecided));
        var pairs = rounds.SelectMany(r => Py.L(r, "views").Select(v => (R: Py.M(r), V: Py.M(v)))).ToList();
        var off = pairs.Where(p => Equals(p.V["status"], "offline")).Select(p => (object?)$"{Py.Str(p.R["step_id"])}/{Py.Str(p.V["stakeholder"])}").ToList();
        if (off.Count > 0)
        {
            string sev = 2 * off.Count >= Math.Max(1, pairs.Count) ? "error" : "warning";
            output.Add(Finding("AUDIT-OFFLINE-VIEWS", sev, off.Count, $"{off.Count} of {pairs.Count} views are offline templates", off));
        }
        var failed = pairs.Where(p => Equals(p.V["status"], "failed"))
            .Select(p => (object?)$"{Py.Str(p.R["step_id"])}/{Py.Str(p.V["stakeholder"])}: {Py.Str(p.V["error"])}").ToList();
        if (failed.Count > 0) output.Add(Finding("AUDIT-FAILED-VIEWS", "warning", failed.Count, $"{failed.Count} views were never valid", failed));
        var fb = new List<object?>();
        foreach (var r in rounds)
        {
            var j = Juror(r);
            var fields = new List<(string, object?)> { ("merge_summary", j["merge_summary"]), ("decision", j["decision"]), ("spec_delta", j["spec_delta"]) };
            foreach (var k in new[] { "resolved", "next_steps", "open_issues" })
            {
                var items = Py.L(j, k);
                for (int i = 0; i < items.Count; i++) fields.Add(($"{k}[{i}]", items[i]));
            }
            foreach (var (name, t) in fields)
            {
                if (Gates.IsFallback(t)) fb.Add($"{Py.Str(Py.M(r)["step_id"])}.{name}: {Py.Head(Py.Str(t), 80)}");
            }
        }
        if (fb.Count > 0) output.Add(Finding("AUDIT-FALLBACK-TEXT", "error", fb.Count, "fallback text where a decision should be", fb));
        // the same devil's-advocate or contribution text in several phases: a template
        var byText = new List<(string Who, string Text, HashSet<string> Steps)>();
        var index = new Dictionary<string, int>(StringComparer.Ordinal);
        foreach (var r in rounds)
        {
            string step = Py.Str(Py.M(r)["step_id"]);
            string baseStep = step.Split('.')[0];
            foreach (var v in Py.L(r, "views"))
            {
                foreach (var f in new[] { "devils_advocate", "contribution" })
                {
                    string sq = Gates.Squash(Py.Str(Py.Get(v, f)));
                    if (Py.Len(sq) < 20) continue;
                    string who = Py.Str(Py.Get(v, "stakeholder"));
                    string key = Py.JsonDumps(Py.A(who, sq), false);
                    if (!index.TryGetValue(key, out int at))
                    {
                        at = byText.Count;
                        index[key] = at;
                        byText.Add((who, sq, new HashSet<string>(StringComparer.Ordinal)));
                    }
                    byText[at].Steps.Add(baseStep);
                }
            }
        }
        var tmpl = byText.Where(e => e.Steps.Count >= 3).Select(e => (object?)$"{e.Who}: {Py.Head(e.Text, 60)}").ToList();
        if (tmpl.Count > 0) output.Add(Finding("AUDIT-TEMPLATE-VIEWS", "error", tmpl.Count, "the same view text appears in 3 or more phases", tmpl));
        var steps = last.SelectMany(r => Py.L(Juror(r), "next_steps")).ToList();
        var gen = steps.Where(Gates.IsGeneric).ToList();
        if (gen.Count > 0)
        {
            string sev = gen.Count == steps.Count ? "error" : "warning";
            output.Add(Finding("AUDIT-GENERIC-STEPS", sev, gen.Count, $"{gen.Count} of {steps.Count} next steps name nothing", gen));
        }
        var ph = pairs.Where(p => Gates.HasPlaceholderCode(p.V["contribution"])).Select(p => (object?)$"{Py.Str(p.R["step_id"])}/{Py.Str(p.V["stakeholder"])}")
            .Concat(rounds.Where(r => Gates.HasPlaceholderCode(Juror(r)["spec_delta"])).Select(r => (object?)$"{Py.Str(Py.M(r)["step_id"])}.spec_delta")).ToList();
        if (ph.Count > 0) output.Add(Finding("AUDIT-PLACEHOLDER-CODE", "warning", ph.Count, "placeholder code presented as content", ph));
        var flags = Py.L(res, "flags");
        if (flags.Count > 0)
        {
            output.Add(Finding("AUDIT-UNSOURCED", "warning", flags.Count, "figures without a source, or unknown sources",
                flags.Select(x =>
                {
                    object? fig = Py.Get(x, "figure");
                    return (object?)$"{Py.Str(Py.Get(x, "step_id"))}/{Py.Str(Py.Get(x, "stakeholder"))}: {Py.Str(Py.Truthy(fig) ? fig : Py.Get(x, "source"))}";
                })));
        }
        var text = new List<string> { Py.Str(res["idea"]) };
        foreach (var r in rounds)
        {
            var j = Juror(r);
            text.Add(Py.Str(j["merge_summary"]));
            text.Add(Py.Str(j["decision"]));
            text.Add(Py.Str(j["spec_delta"]));
            text.AddRange(Py.L(r, "views").Select(v => Py.Str(Py.Get(v, "contribution"))));
        }
        var comp = Gates.ComplianceBlock(string.Join(" ", text));
        if (comp.Count > 0) output.Add(Finding("AUDIT-COMPLIANCE", "error", comp.Count, "compliance rules fired", comp));
        string adapter = Py.Str(res["adapter"]);
        if (Equals(res["synthetic"], true) || SyntheticAdapters.Contains(adapter) || adapter.StartsWith("demo", StringComparison.Ordinal))
            output.Add(Finding("AUDIT-SYNTHETIC", "error", 1, $"adapter {adapter} is synthetic: not a real deliberation"));
        return Report("council", h, output);
    }

    public static JObj AuditStories(object? doc)
    {
        var stories = (doc is JObj ? Py.Get(doc, "stories", Py.A()) : null) as List<object?> ?? new List<object?>();
        var titles = stories.Where(s => s is JObj).Select(s => Py.Get(s, "title", "")).ToList();
        var gen = titles.Where(Gates.IsGeneric).ToList();
        var blocked = stories.Where(s => s is JObj && Equals(Py.Get(s, "status"), "blocked")).Select(s => Py.Get(s, "story_id", "")).ToList();
        var output = new List<JObj>();
        if (gen.Count > 0)
        {
            string sev = gen.Count == titles.Count ? "error" : "warning";
            output.Add(Finding("AUDIT-GENERIC-STEPS", sev, gen.Count, $"{gen.Count} of {titles.Count} story titles name nothing", gen));
        }
        if (blocked.Count > 0) output.Add(Finding("AUDIT-NO-SYNTHESIS", "error", blocked.Count, $"{blocked.Count} stories are blocked (no decision)", blocked));
        if (titles.Count == 0) output.Add(Finding("AUDIT-NO-SYNTHESIS", "error", 0, "no stories"));
        return Report("stories", Py.O("stories", (long)titles.Count), output);
    }

    private static string RowText(object? row)
    {
        if (row is not JObj) return "";
        if (Py.Get(row, "messages") is List<object?> msgs)
            return string.Join(" ", msgs.Where(m => m is JObj && Equals(Py.Get(m, "role"), "assistant")).Select(m => Py.Str(Py.Get(m, "content", ""))));
        return Py.Str(Py.Get(row, "output", ""));
    }

    public static JObj AuditTrainset(IReadOnlyList<object?> rows)
    {
        var bad = new List<object?>();
        var unreadable = new List<object?>();
        var kinds = new SortedDictionary<string, long>(Py.Order);
        for (int i = 0; i < rows.Count; i++)
        {
            var row = rows[i];
            if (row is not JObj || !(Py.Get(row, "output") is string || Py.Get(row, "messages") is List<object?>))
            {
                unreadable.Add($"row {i}: not an object with an output or messages");
                continue;
            }
            var meta = Py.Get(row, "meta", new JObj());
            string k = (meta is JObj ? Py.Get(meta, "kind") : null) as string ?? "unknown";
            kinds[k] = kinds.GetValueOrDefault(k) + 1;
            string t = RowText(row);
            var why = new List<string>();
            if (Gates.IsFallback(t)) why.Add("fallback");
            if (Gates.HasPlaceholderCode(t)) why.Add("placeholder code");
            if (why.Count > 0) bad.Add($"row {i} ({k}): {string.Join(", ", why)}");
        }
        var output = new List<JObj>();
        if (unreadable.Count > 0)
            output.Add(Finding("AUDIT-TRAINSET-UNREADABLE", "error", unreadable.Count, $"{unreadable.Count} of {rows.Count} rows cannot be used", unreadable));
        if (bad.Count > 0)
            output.Add(Finding("AUDIT-TRAINSET-CONTAMINATED", "error", bad.Count, $"{bad.Count} of {rows.Count} rows teach fallback or placeholder output", bad));
        if (rows.Count == 0) output.Add(Finding("AUDIT-TRAINSET-CONTAMINATED", "error", 0, "no rows"));
        return Report("trainset", Py.O("rows", (long)rows.Count, "kinds", kinds.Select(kv => (object?)Py.A(kv.Key, kv.Value)).ToList()), output);
    }

    private static int Order(object? severity) => Equals(severity, "error") ? 0 : Equals(severity, "warning") ? 1 : 2;

    private static JObj Report(string kind, JObj summary, List<JObj> findings)
    {
        var sorted = findings.OrderBy(f => Order(f["severity"])).ThenBy(f => Py.Str(f["code"]), Py.Order).ToList();
        int errors = sorted.Count(f => Equals(f["severity"], "error"));
        return Py.O("kind", kind, "summary", summary, "findings", sorted.Cast<object?>().ToList(),
            "verdict", errors == 0 ? "build-ready" : "not build-ready");
    }

    public const int MaxRows = 200_000;
    public const int MaxChars = 256 * 1024 * 1024;
    public const int MaxLine = 1024 * 1024;
    public const int MaxLines = 1_000_000;

    /// <summary>Parsed rows (null for a line that is not JSON), stopping at the row, size and line budgets.</summary>
    public static List<object?> ReadJsonl(IEnumerable<string> lines)
    {
        long rowsLeft = MaxRows, charsLeft = MaxChars, linesLeft = MaxLines;
        var rows = new List<object?>();
        foreach (var raw in lines)
        {
            charsLeft -= Py.Len(raw);
            linesLeft -= 1;
            if (charsLeft < 0 || rowsLeft <= 0 || linesLeft < 0) break;
            string line = Py.Strip(raw, " \t\r\n");
            if (line.Length == 0) continue;
            rowsLeft -= 1;
            if (Py.Len(line) > MaxLine)
            {
                rows.Add(null);
                continue;
            }
            try { rows.Add(Json.Parse(line)); }
            catch (FormatException) { rows.Add(null); }
        }
        return rows;
    }

    /// <summary>Audit one file: a council JSON (this package or SmartSignal), a stories JSON or a .jsonl trainset.
    /// Zip archives are read by the Python reference only.</summary>
    public static JObj AuditPath(string path)
    {
        if (path.EndsWith(".zip", StringComparison.Ordinal)) throw new ArgumentException("zip trainsets are read by the Python reference only");
        string body = File.ReadAllText(path);
        if (path.EndsWith(".jsonl", StringComparison.Ordinal))
        {
            string norm = body.Replace("\r\n", "\n").Replace("\r", "\n");
            var lines = new List<string>();
            int st = 0;
            for (int k = 0; k < norm.Length; k++)
            {
                if (norm[k] != '\n') continue;
                lines.Add(norm.Substring(st, k + 1 - st));
                st = k + 1;
            }
            if (st < norm.Length) lines.Add(norm.Substring(st));
            return AuditTrainset(ReadJsonl(lines));
        }
        object? doc = Json.Parse(body);
        string? kind = Importer.Detect(doc);
        if (kind == "smartsavant")
        {
            var errs = Schema.CheckCouncil(doc);
            if (errs.Count > 0) throw new ArgumentException("not a valid council file: " + string.Join("; ", errs.Take(3)));
            return AuditResult(Model.ResultFromDict(Py.M(doc)));
        }
        if (kind != null)
        {
            var rep = AuditResult(Importer.FromSmartsignal(doc));
            rep["source"] = kind;
            return rep;
        }
        if (doc is JObj && Py.Get(doc, "stories") is List<object?>) return AuditStories(doc);
        throw new ArgumentException("not a council result, a council export, a stories file or a trainset");
    }
}
