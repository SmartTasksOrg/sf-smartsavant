namespace SmartSavant;

/// <summary>
/// The one boundary between the engine and a model (bindings/council.d.ts). An adapter answers two calls:
/// <c>View</c> returns a ViewAnswer and <c>Merge</c> a MergeAnswer, as JSON data (<see cref="JObj"/>, lists,
/// strings, booleans; any IDictionary with string keys and any IList are accepted too). <c>context</c> is a
/// fresh copy for every call: {idea, iteration, sources, prior, locked}. <c>views</c> are the valid views of
/// the phase as records. The engine copies the answer to plain data, validates it, retries a call that throws
/// or answers invalidly, and records a failure instead of inventing content. The calls are synchronous: an
/// adapter that calls a model over HTTP waits for the answer (for example with
/// <c>GetAwaiter().GetResult()</c>) and the engine calls one at a time.
/// </summary>
public interface ILLMAdapter
{
    /// <summary>Shown in council.json as "adapter".</summary>
    string Name => "adapter";

    /// <summary>True for stubs and demos: their output is never learned from or built on.</summary>
    bool Synthetic => false;

    object? View(string stakeholder, string role, string phaseId, string question, JObj context);

    object? Merge(string phaseId, string question, List<JObj> views, JObj context);
}

/// <summary>What the scripted adapter throws for a {"raise": message} entry; the engine records "AdapterError: message".</summary>
public class AdapterError : Exception
{
    public AdapterError(string message) : base(message) { }
}

/// <summary>What the replay adapter throws for a view or merge the recorded run has no valid answer for.</summary>
public class ReplayError : Exception
{
    public ReplayError(string message) : base(message) { }
}

/// <summary>
/// A deterministic adapter: no model, no network. For tests, demos and ports (the vectors pin its output).
/// It is synthetic: its views are templates, so nothing it produces is a real decision.
/// tag = first 8 hex digits of sha256(UTF-8(stakeholder|phase_id|idea)), written "sv" + tag;
/// why = the first 120 code points of the idea.
/// </summary>
public class StubAdapter : ILLMAdapter
{
    public virtual string Name => "stub";
    public virtual bool Synthetic => true;

    private static string Tag(params string[] parts) => Canonical.Sha256Hex(string.Join("|", parts)).Substring(0, 8);

    public virtual object? View(string stakeholder, string role, string phaseId, string question, JObj context)
    {
        string idea = Py.Str(Py.Get(context, "idea", ""));
        string t = Tag(stakeholder, phaseId, idea);
        return Py.O(
            "w5h1", Py.O("who", stakeholder, "what", $"advance {phaseId}", "where", "the target architecture",
                "when", "this iteration", "why", Py.Head(idea, 120), "how", $"deterministic stub sv{t}"),
            "done_so_far", $"{role} framed {phaseId}.",
            "missing", Py.A($"{phaseId}: target files and modules"),
            "devils_advocate", $"Risk at {phaseId}: an unproven assumption (sv{t}).",
            "contribution", $"{role} proposes the {phaseId} step sv{t}.",
            "sources", Py.A());
    }

    public virtual object? Merge(string phaseId, string question, List<JObj> views, JObj context)
    {
        var resolved = views.Take(5).Select((v, i) => (object?)$"{phaseId} {i + 1}: {Py.Str(v["contribution"])}").ToList();
        var issues = new List<object?>(); // dict.fromkeys: duplicates removed, first occurrence wins
        foreach (var v in views)
        {
            foreach (var m in Py.Iter(Py.Get(v, "missing", Py.A())))
            {
                if (!issues.Any(x => Json.Same(x, m))) issues.Add(m);
            }
        }
        return Py.O(
            "merge_summary", $"Stub juror merged {views.Count} view(s) for {phaseId}.",
            "decision", $"{phaseId}: proceed with the stub plan",
            "resolved", resolved,
            "open_issues", issues.Take(5).ToList(),
            "next_steps", Py.A($"Write the {phaseId} module for the stub plan"),
            "reversals", Py.A(),
            "spec_delta", $"[{phaseId}] stub decision recorded.",
            "loop_recommended", false);
    }
}

/// <summary>
/// An adapter that answers from a script: for tests, the conformance vectors and ports.
/// <c>{"views": {"frame|1|Developer": [ANSWER, ...]}, "merges": {"build|1": [{"raise": "timeout"}, ANSWER]}}</c>,
/// one entry per call, in order. A key not in the script is answered by the stub. An entry {"raise": MESSAGE}
/// throws AdapterError(MESSAGE); anything else is returned as the answer (valid or not: the engine checks).
/// After the list for a key is used up, its last entry repeats. Synthetic is true.
/// </summary>
public class ScriptedAdapter : StubAdapter
{
    private readonly JObj views;
    private readonly JObj merges;
    private readonly Dictionary<string, int> calls = new(StringComparer.Ordinal);

    /// <summary>script: {"views": {...}, "merges": {...}} as parsed JSON (null for none).</summary>
    public ScriptedAdapter(object? script)
    {
        views = Py.Get(script, "views") as JObj ?? new JObj();
        merges = Py.Get(script, "merges") as JObj ?? new JObj();
    }

    public override string Name => "scripted";
    public override bool Synthetic => true;

    private object? Next(JObj table, string key)
    {
        object? seq = table[key];
        if (!Py.Truthy(seq)) return null;
        var items = Py.Iter(seq);
        calls.TryGetValue(key, out int i);
        calls[key] = i + 1;
        object? ans = items[Math.Min(i, items.Count - 1)];
        if (ans is JObj a && a.Count == 1 && a.ContainsKey("raise")) throw new AdapterError(Py.Str(a["raise"]));
        return Py.Clone(ans);
    }

    public override object? View(string stakeholder, string role, string phaseId, string question, JObj context)
    {
        string key = $"{phaseId}|{Py.Str(Py.Get(context, "iteration", 1L))}|{stakeholder}";
        return views.ContainsKey(key) ? Next(views, key) : base.View(stakeholder, role, phaseId, question, context);
    }

    public override object? Merge(string phaseId, string question, List<JObj> views, JObj context)
    {
        string key = $"{phaseId}|{Py.Str(Py.Get(context, "iteration", 1L))}";
        return merges.ContainsKey(key) ? Next(merges, key) : base.Merge(phaseId, question, views, context);
    }
}

/// <summary>
/// Replays the answers of a recorded run (a council.json written by an engine of this package). Views and
/// merges whose status was not ok / full / thin are replayed as failures (the adapter throws ReplayError),
/// so the replayed run has the same health as the original.
/// </summary>
public class ReplayAdapter : ILLMAdapter
{
    private readonly bool synthetic;
    private readonly Dictionary<string, object?> views = new(StringComparer.Ordinal);
    private readonly Dictionary<string, object?> merges = new(StringComparer.Ordinal);

    /// <summary>recorded: a parsed council object.</summary>
    public ReplayAdapter(JObj recorded)
    {
        synthetic = Py.Truthy(Py.Get(recorded, "synthetic", false));
        foreach (var r in Py.Iter(Py.Get(recorded, "rounds", Py.A())))
        {
            object? step = Py.Get(r, "step_id"), it = Py.Get(r, "iteration");
            foreach (var v in Py.Iter(Py.Get(r, "views", Py.A())))
            {
                views.TryAdd(Key(step, it, Py.Get(v, "stakeholder")), v);
            }
            merges[Key(step, it)] = Py.Get(r, "juror", new JObj());
        }
    }

    /// <summary>Reads a council.json file.</summary>
    public static ReplayAdapter FromFile(string path) => new(Py.M(Json.Parse(File.ReadAllText(path))));

    private static string Key(params object?[] parts) => Py.JsonDumps(parts.Select(p => Py.IsInt(p) ? (object?)Py.BigInt(p!) : p).ToList(), false);

    public string Name => "replay";
    public bool Synthetic => synthetic;

    public object? View(string stakeholder, string role, string phaseId, string question, JObj context)
    {
        views.TryGetValue(Key(phaseId, Py.Get(context, "iteration", 1L), stakeholder), out var v);
        if (v == null || !Equals(Py.Get(v, "status"), "ok")) throw new ReplayError($"no recorded valid view for {stakeholder} at {phaseId}");
        return Py.Clone(Py.O("w5h1", Py.Get(v, "w5h1"), "done_so_far", Py.Get(v, "done_so_far"), "missing", Py.Get(v, "missing"),
            "devils_advocate", Py.Get(v, "devils_advocate"), "contribution", Py.Get(v, "contribution"), "sources", Py.Get(v, "sources")));
    }

    public object? Merge(string phaseId, string question, List<JObj> views, JObj context)
    {
        merges.TryGetValue(Key(phaseId, Py.Get(context, "iteration", 1L)), out var m);
        object? st = Py.Get(m, "synthesis_status");
        if (m == null || !(Equals(st, "full") || Equals(st, "thin"))) throw new ReplayError($"no recorded valid merge for {phaseId}");
        var output = new JObj();
        foreach (var k in Schema.MergeKeys) output[k] = Py.Get(m, k);
        return Py.Clone(output);
    }
}
