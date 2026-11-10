using System.Collections;

namespace SmartSavant;

/// <summary>
/// The council engine: roster, phases, stakeholder views, juror merge, decision tree.
/// <c>JObj res = new Council("...", new StubAdapter()).Run();</c>
/// Every answer is copied to plain data, validated (<see cref="Schema.CheckView"/> / <see cref="Schema.CheckMerge"/>)
/// and retried; one that still fails is recorded as failed with an error, never filled with invented text.
/// Deterministic for a deterministic adapter. Not thread-safe; the adapter is called one call at a time.
/// </summary>
public sealed class Council
{
    public static readonly IReadOnlyList<string> DefaultRoster = new[] { "Developer", "UX/UI", "Product Owner", "Architecture/Infra" };

    public static readonly IReadOnlyList<(string StepId, string Title, string Question)> DefaultPhases = new[]
    {
        ("frame", "Frame", "What is the problem, who are the users, and the jobs-to-be-done?"),
        ("concept", "Concept", "What is the solution direction and the wedge?"),
        ("design", "Design", "What are the user flow, data flow, and experience?"),
        ("build", "Build", "What is the technical approach?"),
        ("integrate", "Integrate", "How does it fit the target architecture?"),
        ("measure", "Measure", "What are the KPIs and success metrics?"),
        ("spec", "Spec", "Assemble the final document."),
    };

    private readonly string idea;
    private readonly ILLMAdapter adapter;
    private readonly List<string> roster;
    private readonly IReadOnlyList<(string StepId, string Title, string Question)> phases;
    private readonly List<object?> sources;
    private readonly int maxLoops;
    private readonly int retries;

    /// <param name="idea">the idea (non-blank)</param>
    /// <param name="adapter">the model access</param>
    /// <param name="roster">stakeholder names (null or empty: <see cref="DefaultRoster"/>)</param>
    /// <param name="phases">phases as (step_id, title, question) tuples, [step_id, title, question] lists or
    /// {step_id, title, question} objects (null or empty: <see cref="DefaultPhases"/>)</param>
    /// <param name="sources">source ids views may cite</param>
    /// <param name="maxLoops">1 to 10</param>
    /// <param name="retries">0 to 5</param>
    public Council(string idea, ILLMAdapter adapter, IEnumerable? roster = null, IEnumerable? phases = null,
                   IEnumerable? sources = null, int maxLoops = 1, int retries = 1)
    {
        if (idea == null || Gates.Blank(idea)) throw new ArgumentException("the idea must be a non-empty string");
        var rosterIn = roster?.Cast<object?>().ToList() ?? new List<object?>();
        var phasesIn = phases?.Cast<object?>().ToList() ?? new List<object?>();
        var sourcesIn = sources?.Cast<object?>().ToList() ?? new List<object?>();
        var texts = new List<object?> { idea };
        texts.AddRange(rosterIn);
        texts.AddRange(sourcesIn);
        foreach (var p in phasesIn) texts.AddRange(PhaseParts(p));
        if (texts.Any(t => t is string s && Py.HasLone(s)))
            throw new ArgumentException("the idea, roster, phases and sources must not contain unpaired surrogates");
        this.adapter = adapter ?? throw new ArgumentException("adapter must be an LLMAdapter");
        if (rosterIn.Count > 0)
        {
            if (!rosterIn.All(r => r is string s && !Gates.Blank(s)) || rosterIn.Distinct().Count() != rosterIn.Count)
                throw new ArgumentException("roster names must be unique non-empty strings");
            this.roster = rosterIn.Cast<string>().ToList();
        }
        else this.roster = DefaultRoster.ToList();
        this.phases = phasesIn.Count > 0 ? CheckPhases(phasesIn) : DefaultPhases;
        if (maxLoops < 1 || maxLoops > 10) throw new ArgumentException("max_loops is an integer from 1 to 10");
        if (retries < 0 || retries > 5) throw new ArgumentException("retries is an integer from 0 to 5");
        this.idea = idea;
        this.sources = sourcesIn;
        this.maxLoops = maxLoops;
        this.retries = retries;
    }

    private static List<object?> PhaseParts(object? p) => p switch
    {
        JObj o => new List<object?> { o["step_id"], o["title"], o["question"] },
        ValueTuple<string, string, string> t => new List<object?> { t.Item1, t.Item2, t.Item3 },
        IList l => l.Cast<object?>().ToList(),
        _ => new List<object?>(),
    };

    /// <summary>(step_id, title, question) with unique step ids and non-blank strings, or ArgumentException.</summary>
    public static List<(string StepId, string Title, string Question)> CheckPhases(IEnumerable phases)
    {
        var output = new List<(string, string, string)>();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        foreach (var p in phases)
        {
            bool shape = p is JObj or IList or ValueTuple<string, string, string>;
            var t = PhaseParts(p);
            if (!shape || t.Count != 3 || !t.All(x => x is string s && !Gates.Blank(s)))
                throw new ArgumentException("each phase is (step_id, title, question), three non-blank strings");
            string id = (string)t[0]!;
            if (!seen.Add(id)) throw new ArgumentException($"duplicate step_id {id}");
            output.Add((id, (string)t[1]!, (string)t[2]!));
        }
        if (output.Count == 0) throw new ArgumentException("at least one phase");
        return output;
    }

    /// <summary>Phases from a phases file in the format of spec/phases.json.</summary>
    public static List<(string StepId, string Title, string Question)> LoadPhases(string path)
    {
        object? doc = Json.Parse(File.ReadAllText(path));
        if (Py.Get(doc, "phases") is not List<object?> ph || ph.Count == 0)
            throw new ArgumentException("a phases file is {\"phases\": [{\"step_id\", \"title\", \"question\"}, ...]}");
        if (!ph.All(p => p is JObj)) throw new ArgumentException("each phase is an object {\"step_id\", \"title\", \"question\"}");
        return CheckPhases(ph);
    }

    private const string NotPrintable = "(message not printable)";

    /// <summary>
    /// "Name: message" as the Python reference writes it: the exception's class name, its message
    /// ("(message not printable)" when reading it throws), lone surrogates as '?', secrets redacted, then cut
    /// to 500 code points. Never throws.
    /// </summary>
    public static string ErrorText(Exception e)
    {
        string name;
        try { name = e.GetType().Name; }
        catch (Exception) { name = "Exception"; }
        string msg;
        try { msg = e.Message ?? ""; }
        catch (Exception) { msg = NotPrintable; }
        msg = Py.ReplaceLone(msg, "?");
        return Py.Head(Gates.Redact(name + ": " + msg), 500);
    }

    /// <summary>(answer, "") or (null, error) after 1 + retries attempts.</summary>
    private (object? Answer, string Error) Ask(Func<object?> call, Func<object?, List<string>> check)
    {
        string err = "";
        for (int n = 0; n < 1 + retries; n++)
        {
            object? ans;
            try
            {
                ans = Schema.Plain(call()); // validate exactly what will be used
            }
            catch (NotPlainData e)
            {
                err = Py.Head("invalid: " + e.Message, 500);
                continue;
            }
            catch (Exception e) // an adapter failure is data, not a crash
            {
                err = ErrorText(e);
                continue;
            }
            var problems = check(ans);
            if (problems.Count == 0) return (ans, "");
            err = Py.Head("invalid: " + string.Join("; ", problems.Take(5).Select(p => Py.Head(p, 200))), 500);
        }
        return (null, err);
    }

    /// <summary>Runs the council; returns the CouncilResult as an ordered <see cref="JObj"/> (plain JSON data).</summary>
    public JObj Run()
    {
        var locked = new List<object?>();
        var rounds = new List<object?>();
        var tree = new List<object?>();
        var flags = new List<object?>();
        var res = Model.CouncilResult(Py.O("idea", idea, "adapter", adapter.Name ?? "adapter", "synthetic", adapter.Synthetic,
            "stakeholders", roster.Cast<object?>().ToList(), "sources", sources.ToList(), "rounds", rounds,
            "decision_tree", tree, "locked_decisions", locked, "flags", flags));
        var prior = new List<object?>();
        foreach (var (stepId, title, question) in phases)
        {
            long it = 0;
            while (true)
            {
                it += 1;
                var ctx = Py.O("idea", idea, "iteration", it, "sources", sources.ToList(), "prior", prior, "locked", locked);
                var views = new List<object?>();
                var okViews = new List<JObj>();
                foreach (var role in roster)
                {
                    var (got, err) = Ask(() => adapter.View(role, role, stepId, question, Py.M(Py.Clone(ctx))), Schema.CheckView);
                    if (got == null)
                    {
                        views.Add(Model.StakeholderView(Py.O("stakeholder", role, "role", role, "status", "failed", "error", err)));
                        continue;
                    }
                    var raw = Schema.NormalizeView(Py.M(got));
                    var v = Model.StakeholderView(Py.O("stakeholder", role, "role", role, "status", "ok", "w5h1", raw["w5h1"],
                        "done_so_far", raw["done_so_far"], "missing", Py.L(raw["missing"]).ToList(),
                        "devils_advocate", raw["devils_advocate"], "contribution", raw["contribution"],
                        "sources", Py.L(raw["sources"]).ToList()));
                    views.Add(v);
                    okViews.Add(v);
                    var fv = new JObj();
                    foreach (var kv in raw) fv[kv.Key] = kv.Value;
                    fv["stakeholder"] = role;
                    foreach (var fl in Gates.FlagUnsourced(fv, sources))
                    {
                        fl["step_id"] = stepId;
                        fl["iteration"] = it;
                        flags.Add(fl);
                    }
                }
                JObj juror;
                if (okViews.Count == 0)
                {
                    juror = Model.JurorMerge(Py.O("synthesis_status", "no_views", "error", "no valid view to merge"));
                }
                else
                {
                    var (m, err) = Ask(() => adapter.Merge(stepId, question, okViews.Select(x => Py.M(Py.Clone(x))).ToList(), Py.M(Py.Clone(ctx))),
                        Schema.CheckMerge);
                    if (m == null)
                    {
                        juror = Model.JurorMerge(Py.O("synthesis_status", "failed", "error", err));
                    }
                    else
                    {
                        var a = Py.M(m);
                        juror = Model.JurorMerge(Py.O("synthesis_status", "full", "merge_summary", a["merge_summary"], "decision", a["decision"],
                            "resolved", Py.L(a["resolved"]).ToList(), "open_issues", Py.L(a["open_issues"]).ToList(),
                            "next_steps", Py.L(a["next_steps"]).ToList(),
                            "reversals", a.ContainsKey("reversals") ? Py.L(a["reversals"]).ToList() : new List<object?>(),
                            "spec_delta", a["spec_delta"], "loop_recommended", a["loop_recommended"]));
                        if (!Gates.SynthesisOk(juror)) juror["synthesis_status"] = "thin";
                    }
                }
                rounds.Add(Model.CouncilRound(stepId, title, it, question, views, juror));
                tree.Add(Model.DecisionNode(stepId, it, okViews.Select(v => v["contribution"]).ToList(), juror["decision"],
                    Py.L(juror, "next_steps").ToList()));
                if (Equals(juror["synthesis_status"], "full"))
                {
                    foreach (var r in Py.L(juror, "resolved")) if (!locked.Any(x => Equals(x, r))) locked.Add(r);
                    prior = new List<object?>(prior) { Py.O("step", stepId, "decision", juror["decision"]) };
                }
                if (!Equals(juror["loop_recommended"], true) || it >= maxLoops) break;
            }
        }
        return res;
    }

    /// <summary>Counts for a run: views by status, merges by synthesis status, flags, and whether every phase's
    /// last iteration reached a full synthesis (decided).</summary>
    public static JObj Health(JObj res)
    {
        var views = new SortedDictionary<string, long>(Py.Order);
        var merges = new SortedDictionary<string, long>(Py.Order);
        var last = new JObj();
        foreach (var r in Py.L(res, "rounds"))
        {
            foreach (var v in Py.L(r, "views"))
            {
                string st = Py.S(v, "status");
                views[st] = views.GetValueOrDefault(st) + 1;
            }
            string js = Py.S(Py.M(r)["juror"], "synthesis_status");
            merges[js] = merges.GetValueOrDefault(js) + 1;
            last[Py.Str(Py.M(r)["step_id"])] = js;
        }
        var vo = new JObj();
        foreach (var kv in views) vo[kv.Key] = kv.Value;
        var mo = new JObj();
        foreach (var kv in merges) mo[kv.Key] = kv.Value;
        long decided = last.Values.Count(s => Equals(s, "full"));
        return Py.O("views", vo, "merges", mo, "phases", (long)last.Count, "phases_decided", decided,
            "decided", last.Count > 0 && decided == last.Count, "flags", (long)Py.L(res, "flags").Count, "synthetic", res["synthetic"]);
    }
}
