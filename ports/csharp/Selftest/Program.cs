// Checks of what the vectors do not cover: a hand-written adapter, exception text, answers that are not
// plain data, replay of a recorded run, refused inputs, Python number and string formatting, and the ASCII
// regex semantics. Usage: dotnet run -c Release --project Selftest ; exits 1 on a failure.
using SmartSavant;

int failed = 0;

void Eq(string name, object? got, object? want)
{
    if (Json.Same(got, want)) return;
    failed++;
    Console.WriteLine($"{name}: got {Py.Repr(got)}, want {Py.Repr(want)}");
}

void Throws(string name, Action a, string message)
{
    try
    {
        a();
        failed++;
        Console.WriteLine($"{name}: did not throw");
    }
    catch (ArgumentException e)
    {
        Eq(name, e.Message, message);
    }
}

object? FirstViewError(ILLMAdapter a)
{
    var res = new Council("an idea", a, new[] { "Dev" }, new[] { ("frame", "Frame", "Q?") }, null, 1, 0).Run();
    return Py.Get(Py.L(Py.L(res, "rounds")[0], "views")[0], "error");
}

Eq("num", Py.A(Py.Num(1e16), Py.Num(1.5e-05), Py.Num(0.1), Py.Num(123.456), Py.Num(1e22), Py.Num(-0.0), Py.Num(5e-324),
        Py.Num(1.0), Py.Num(1e-4), Py.Num(12345678901234567.0), Py.Num(2.5)),
    Py.A("1e+16", "1.5e-05", "0.1", "123.456", "1e+22", "-0.0", "5e-324", "1.0", "0.0001", "1.2345678901234568e+16", "2.5"));
Eq("repr", Py.A(Py.Repr("a'b"), Py.Repr("x\u0001\u2028\ud83d\ude00\u00a0\u00e9")),
    Py.A("\"a'b\"", "'x\\x01\\u2028\ud83d\ude00\\xa0\u00e9'"));
Eq("kelvin sign is not k", Gates.IsFallback("fallbac\u212a template"), false);
Eq("$ before U+2028", Gates.IsGeneric("tbd\u2028"), false);
Eq("$ before a final newline", Gates.IsGeneric("tbd\n"), true);
Eq("redact", Gates.Redact("Token: abc  BEARER abcdefghij"), "Token: ***  BEARER ***");
Eq("no unicode digits", Gates.Figures("\u0663.5 s and 3.5 s"), Py.A("3.5 s"));
Eq("no unicode spaces", Gates.Squash("a\u00a0 b\u3000"), "a\u00a0 b\u3000");
Eq("lone surrogates around a dropped control character stay apart",
    Schema.CheckView(Py.O("\ud83d\u0001\ude00x", 1L, "a\ud800", 2L)), Py.A("unknown keys (2): a\ufffd, \ufffd\ufffdx", "missing keys: contribution, devils_advocate, done_so_far, missing, sources, w5h1"));
Eq("exception text", FirstViewError(new Flaky(new InvalidOperationException("boom api_key=hunter2"))), "InvalidOperationException: boom api_key=***");
Eq("lone surrogate in a message", FirstViewError(new Flaky(new AdapterError("bad \ud800 text"))), "AdapterError: bad ? text");
Eq("not plain data", FirstViewError(new Flaky(Py.O("w5h1", new DateTime(0)))), "invalid: DateTime is not JSON data");
Eq("not an object", FirstViewError(new Flaky(Py.A())), "invalid: a view must be a JSON object");
Eq("a dictionary is accepted and checked", FirstViewError(new Flaky(new Dictionary<string, object?> { ["w5h1"] = 1 })),
    "invalid: missing keys: contribution, devils_advocate, done_so_far, missing, sources");
Throws("lone surrogate in the idea", () => new Council("idea \udc00", new StubAdapter()),
    "the idea, roster, phases and sources must not contain unpaired surrogates");
Throws("duplicate roster", () => new Council("idea", new StubAdapter(), new[] { "A", "A" }), "roster names must be unique non-empty strings");
Throws("duplicate step", () => new Council("idea", new StubAdapter(), null, new[] { ("a", "A", "?"), ("a", "B", "?") }), "duplicate step_id a");
Throws("max_loops", () => new Council("idea", new StubAdapter(), maxLoops: 0), "max_loops is an integer from 1 to 10");

// a recorded run replays to the same rounds; its JSON text reads back as the same value
var rec = new Council("Replay me: 30% faster builds", new StubAdapter()).Run();
Eq("council file is valid", Schema.CheckCouncil(rec), Py.A());
var back = Py.M(Json.Parse(Py.JsonDumps(rec, false)));
Eq("json round trip", back, rec);
var again = new Council("Replay me: 30% faster builds", new ReplayAdapter(back)).Run();
Eq("replay rounds", again["rounds"], rec["rounds"]);
Eq("replay adapter name", again["adapter"], "replay");
Eq("replay synthetic", again["synthetic"], true);
Eq("audit of a stub run", Audit.AuditResult(rec)["verdict"], "not build-ready");
var bad = new Council("idea", new Flaky(new AdapterError("down")), new[] { "Dev" }, new[] { ("frame", "Frame", "Q?") }, null, 1, 0).Run();
var rebad = new Council("idea", new ReplayAdapter(bad), new[] { "Dev" }, new[] { ("frame", "Frame", "Q?") }, null, 1, 0).Run();
Eq("replayed failure", Py.Get(Py.L(Py.L(rebad, "rounds")[0], "views")[0], "error"), "ReplayError: no recorded valid view for Dev at frame");
Eq("replayed failure health", Council.Health(rebad)["merges"], Py.O("no_views", 1L));
Eq("readJsonl", Audit.ReadJsonl(new[] { "{\"output\": \"x\"}\n", "\n", "not json\n" }), Py.A(Py.O("output", "x"), null));

// optional: float formatting against Python repr() on pairs written by the caller (hex input, expected repr)
if (args.Length == 2)
{
    var inp = File.ReadAllLines(args[0]);
    var want = File.ReadAllLines(args[1]);
    int bad2 = 0;
    for (int i = 0; i < inp.Length; i++)
    {
        double d = BitConverter.Int64BitsToDouble(long.Parse(inp[i], System.Globalization.CultureInfo.InvariantCulture));
        if (Py.Num(d) != want[i] && bad2++ < 10) Console.WriteLine($"num {inp[i]}: {Py.Num(d)} != {want[i]}");
    }
    if (bad2 > 0) failed++;
    Console.WriteLine($"num fuzz: {bad2} of {inp.Length} differ");
}

Console.WriteLine(failed == 0 ? "csharp: selftest passed" : $"csharp: selftest FAILED ({failed})");
return failed == 0 ? 0 : 1;

/// <summary>An adapter as an application writes one: it fails (or answers badly) in the views and merges like the stub.</summary>
sealed class Flaky : ILLMAdapter
{
    private readonly StubAdapter stub = new();
    private readonly object? viewAnswer;

    public Flaky(object? viewAnswer) { this.viewAnswer = viewAnswer; }

    public string Name => "flaky";

    public object? View(string stakeholder, string role, string phaseId, string question, JObj context) =>
        viewAnswer is Exception e ? throw e : viewAnswer;

    public object? Merge(string phaseId, string question, List<JObj> views, JObj context) => stub.Merge(phaseId, question, views, context);
}
