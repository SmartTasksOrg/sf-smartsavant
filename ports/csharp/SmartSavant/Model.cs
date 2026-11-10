namespace SmartSavant;

/// <summary>
/// The council data model as ordered <see cref="JObj"/> records with the fields in the order of the Python
/// dataclasses (src/sf_smartsavant/model.py and bindings/council.d.ts). A result is plain JSON data: the
/// canonical writer hashes it and <see cref="Py.JsonDumps"/> writes a council.json the Python reference reads.
/// view.status is ok, failed or offline; juror.synthesis_status is full, thin, recovered, failed, offline or no_views.
/// </summary>
public static class Model
{
    public static readonly IReadOnlyList<string> ViewStatuses = new[] { "ok", "failed", "offline" };
    public static readonly IReadOnlyList<string> SynthesisStatuses = new[] { "full", "thin", "recovered", "failed", "offline", "no_views" };

    private static object? Or(JObj f, string k, object? d) => f[k] ?? d;

    /// <summary>{who, what, where, when, why, how}, "" for each field not given.</summary>
    public static JObj W5h1(JObj? f)
    {
        var o = new JObj();
        foreach (var k in Schema.W5h1Keys) o[k] = f?[k] ?? "";
        return o;
    }

    /// <summary>A stakeholder view record; f gives the fields (stakeholder and role are required).</summary>
    public static JObj StakeholderView(JObj f) => Py.O(
        "stakeholder", f["stakeholder"], "role", f["role"], "status", Or(f, "status", "ok"), "w5h1", W5h1(f["w5h1"] as JObj),
        "done_so_far", Or(f, "done_so_far", ""), "missing", Or(f, "missing", new List<object?>()),
        "devils_advocate", Or(f, "devils_advocate", ""), "contribution", Or(f, "contribution", ""),
        "sources", Or(f, "sources", new List<object?>()), "error", Or(f, "error", ""));

    /// <summary>A juror merge record with every default filled in.</summary>
    public static JObj JurorMerge(JObj f) => Py.O(
        "synthesis_status", Or(f, "synthesis_status", "full"), "merge_summary", Or(f, "merge_summary", ""),
        "decision", Or(f, "decision", ""), "resolved", Or(f, "resolved", new List<object?>()),
        "open_issues", Or(f, "open_issues", new List<object?>()), "next_steps", Or(f, "next_steps", new List<object?>()),
        "reversals", Or(f, "reversals", new List<object?>()), "spec_delta", Or(f, "spec_delta", ""),
        "loop_recommended", Or(f, "loop_recommended", false), "error", Or(f, "error", ""));

    public static JObj CouncilRound(object? stepId, object? stepTitle, long iteration, object? question, List<object?> views, JObj juror) =>
        Py.O("step_id", stepId, "step_title", stepTitle, "iteration", iteration, "question", question, "views", views, "juror", juror);

    public static JObj DecisionNode(object? stepId, long iteration, List<object?> branches, object? decision, List<object?> nextSteps) =>
        Py.O("step_id", stepId, "iteration", iteration, "branches", branches, "decision", decision, "next_steps", nextSteps);

    /// <summary>A council result record with every default filled in.</summary>
    public static JObj CouncilResult(JObj f) => Py.O(
        "schema", Or(f, "schema", "smartsavant.council/1"), "idea", Or(f, "idea", ""), "adapter", Or(f, "adapter", ""),
        "synthetic", Or(f, "synthetic", false), "stakeholders", Or(f, "stakeholders", new List<object?>()),
        "sources", Or(f, "sources", new List<object?>()), "rounds", Or(f, "rounds", new List<object?>()),
        "decision_tree", Or(f, "decision_tree", new List<object?>()),
        "locked_decisions", Or(f, "locked_decisions", new List<object?>()), "flags", Or(f, "flags", new List<object?>()));

    /// <summary>A council.json object (check it with <see cref="Schema.CheckCouncil"/> first) rebuilt with every default filled in.</summary>
    public static JObj ResultFromDict(JObj d)
    {
        var copy = Py.M(Py.Clone(d));
        var rounds = new List<object?>();
        foreach (var r in (copy["rounds"] as List<object?>) ?? new())
        {
            var views = ((Py.Get(r, "views") as List<object?>) ?? new()).Select(v => (object?)StakeholderView(Py.M(v))).ToList();
            rounds.Add(CouncilRound(Py.Get(r, "step_id"), Py.Get(r, "step_title"), (long)Py.BigInt(Py.Get(r, "iteration")!),
                Py.Get(r, "question"), views, JurorMerge(Py.Get(r, "juror") as JObj ?? new JObj())));
        }
        var tree = new List<object?>();
        foreach (var n in (copy["decision_tree"] as List<object?>) ?? new())
        {
            tree.Add(DecisionNode(Py.Get(n, "step_id"), (long)Py.BigInt(Py.Get(n, "iteration")!),
                Py.Get(n, "branches") as List<object?> ?? new(), Py.Get(n, "decision") ?? "", Py.Get(n, "next_steps") as List<object?> ?? new()));
        }
        copy["rounds"] = rounds;
        copy["decision_tree"] = tree;
        return CouncilResult(copy);
    }
}
