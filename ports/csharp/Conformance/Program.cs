// Runs spec/conformance against this port, check for check as src/sf_smartsavant/conformance.py does.
// Usage: dotnet run -c Release --project Conformance -- [vector dir]
// Prints the failures and a last line "csharp: N/N vectors passed"; exits 1 on any failure.
using SmartSavant;

string dir = args.Length > 0 ? args[0] : Path.Combine("..", "..", "spec", "conformance");
int total = 0;
var fails = new List<string>();

string Show(object? v)
{
    string s;
    try { s = Py.JsonDumps(v, true); }
    catch (Exception) { s = v?.ToString() ?? "null"; }
    return Py.Head(s, 300);
}

void Check(string name, Func<object?> got, object? want)
{
    total++;
    object? g;
    try { g = got(); }
    catch (Exception e)
    {
        fails.Add($"{name}: threw {Council.ErrorText(e)}");
        return;
    }
    if (!Json.Same(g, want)) fails.Add($"{name}: got {Show(g)}, want {Show(want)}");
}

JObj Load(string name) => Py.M(Json.Parse(File.ReadAllText(Path.Combine(dir, name))));
List<object?> Cases(JObj doc, string key) => Py.L(doc["cases"], key);

foreach (var c in Py.L(Load("canonical.json"), "cases"))
{
    Check("canonical/" + Py.S(c, "name"), () =>
    {
        string t = Canonical.Dumps(Py.Get(c, "input"));
        return Py.A(t, Canonical.Sha256Hex(t));
    }, Py.A(Py.Get(c, "canonical"), Py.Get(c, "sha256")));
}
var fig = Py.L(Load("figures.json"), "cases");
for (int i = 0; i < fig.Count; i++)
{
    var c = fig[i];
    Check($"figures/{i}", () => Gates.Figures(Py.Get(c, "text")), Py.Get(c, "figures"));
}
var g = Load("gates.json");
foreach (var c in Cases(g, "flag_unsourced"))
    Check("flag_unsourced/" + Py.S(c, "name"), () => Gates.FlagUnsourced(Py.Get(c, "view"), Py.Get(c, "allowed")), Py.Get(c, "flags"));
var fns = new (string, Func<object?, object?>)[]
{
    ("is_generic", t => Gates.IsGeneric(t)),
    ("is_fallback", t => Gates.IsFallback(t)),
    ("has_placeholder_code", t => Gates.HasPlaceholderCode(t)),
    ("compliance_block", t => Gates.ComplianceBlock(t)),
    ("squash", t => Gates.Squash((string?)t)),
};
foreach (var (fn, f) in fns)
{
    var cs = Cases(g, fn);
    for (int i = 0; i < cs.Count; i++)
    {
        var c = cs[i];
        Check($"{fn}/{i}", () => f(Py.Get(c, "text")), Py.Get(c, "result"));
    }
}
foreach (var c in Cases(g, "synthesis_ok"))
    Check("synthesis_ok/" + Py.S(c, "name"), () => Gates.SynthesisOk(Py.Get(c, "merge")), Py.Get(c, "result"));
var s = Load("schema.json");
foreach (var c in Cases(s, "view")) Check("schema/view/" + Py.S(c, "name"), () => Schema.CheckView(Py.Get(c, "input")), Py.Get(c, "errors"));
foreach (var c in Cases(s, "merge")) Check("schema/merge/" + Py.S(c, "name"), () => Schema.CheckMerge(Py.Get(c, "input")), Py.Get(c, "errors"));
foreach (var c in Py.L(Load("runs.json"), "cases"))
{
    string name = Py.S(c, "name");
    var expect = Py.M(Py.Get(c, "expect"));
    JObj res;
    try
    {
        res = new Council(Py.S(c, "idea"), new ScriptedAdapter(Py.Get(c, "script")),
            Py.Get(c, "roster") as List<object?>, Py.Get(c, "phases") as List<object?>, Py.Get(c, "sources") as List<object?>,
            (int)Py.BigInt(Py.Get(c, "max_loops")!), (int)Py.BigInt(Py.Get(c, "retries")!)).Run();
    }
    catch (Exception e)
    {
        foreach (var f in expect.Keys)
        {
            total++;
            fails.Add($"runs/{name}/{f}: the run threw {Council.ErrorText(e)}");
        }
        continue;
    }
    string k = Py.S(c, "project_key");
    var got = new Dictionary<string, Func<object?>>
    {
        ["council"] = () => res, ["digest"] = () => Exporters.RunDigest(res), ["health"] = () => Council.Health(res),
        ["stories"] = () => Exporters.ToStories(res, k), ["spec_md"] = () => Exporters.ToSpecMd(res),
        ["mermaid"] = () => Exporters.ToMermaid(res), ["trainset"] = () => Exporters.ToTrainset(res),
        ["trainset_all"] = () => Exporters.ToTrainset(res, true), ["audit"] = () => Audit.AuditResult(res),
    };
    foreach (var e in expect)
    {
        if (!got.TryGetValue(e.Key, out var f))
        {
            total++;
            fails.Add($"runs/{name}/{e.Key}: unknown field");
            continue;
        }
        Check($"runs/{name}/{e.Key}", f, e.Value);
    }
}
var a = Load("audit.json");
foreach (var c in Cases(a, "import")) Check("import/" + Py.S(c, "name"), () => Importer.FromSmartsignal(Py.Get(c, "input")), Py.Get(c, "council"));
foreach (var c in Cases(a, "council"))
    Check("audit/council/" + Py.S(c, "name"), () => Audit.AuditResult(Importer.FromSmartsignal(Py.Get(c, "input"))), Py.Get(c, "report"));
foreach (var c in Cases(a, "stories")) Check("audit/stories/" + Py.S(c, "name"), () => Audit.AuditStories(Py.Get(c, "input")), Py.Get(c, "report"));
foreach (var c in Cases(a, "trainset")) Check("audit/trainset/" + Py.S(c, "name"), () => Audit.AuditTrainset(Py.L(Py.Get(c, "input"))), Py.Get(c, "report"));

foreach (var f in fails) Console.WriteLine(f);
Console.WriteLine(fails.Count == 0 ? $"csharp: {total}/{total} vectors passed" : $"csharp: {total - fails.Count}/{total} vectors passed ({fails.Count} failed)");
return fails.Count == 0 ? 0 : 1;
