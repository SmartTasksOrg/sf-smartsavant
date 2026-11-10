package cloud.smarttasks.smartsavant;

import java.util.ArrayList;
import java.util.Date;
import java.util.List;
import java.util.Map;

/**
 * Checks of what the vectors do not cover: a hand-written adapter, exception text, answers that are not
 * plain data, replay of a recorded run, refused inputs and Python number and string formatting.
 * Usage: {@code java -cp build/classes cloud.smarttasks.smartsavant.Selftest}; exits 1 on a failure.
 */
public final class Selftest {
    private static int failed;

    private static void eq(String name, Object got, Object want) {
        if (!Json.same(got, want)) {
            failed++;
            System.out.println(name + ": got " + Py.repr(got) + ", want " + Py.repr(want));
        }
    }

    private static void throwsIae(String name, Runnable r, String message) {
        try {
            r.run();
            failed++;
            System.out.println(name + ": did not throw");
        } catch (IllegalArgumentException e) {
            eq(name, e.getMessage(), message);
        }
    }

    /** An adapter as an application writes one: here it fails in the views and answers the merge from the stub. */
    static final class Flaky implements LLMAdapter {
        private final StubAdapter stub = new StubAdapter();
        private final Object viewAnswer;

        Flaky(Object viewAnswer) { this.viewAnswer = viewAnswer; }

        @Override public String name() { return "flaky"; }

        @Override
        public Object view(String stakeholder, String role, String phaseId, String question, Map<String, Object> context) {
            if (viewAnswer instanceof RuntimeException e) throw e;
            return viewAnswer;
        }

        @Override
        public Object merge(String phaseId, String question, List<Map<String, Object>> views, Map<String, Object> context) {
            return stub.merge(phaseId, question, views, context);
        }
    }

    private static Object firstViewError(LLMAdapter a) {
        Map<String, Object> res = new Council("an idea", a, List.of("Dev"), List.of(List.of("frame", "Frame", "Q?")), null, 1, 0).run();
        return Py.get(Py.l(Py.l(res, "rounds").get(0), "views").get(0), "error");
    }

    public static void main(String[] args) {
        eq("num", List.of(Py.num(1e16), Py.num(1.5e-05), Py.num(0.1), Py.num(123.456), Py.num(1e22), Py.num(-0.0),
                Py.num(5e-324), Py.num(1.0), Py.num(1e-4), Py.num(12345678901234567.0), Py.num(2.5)),
            List.of("1e+16", "1.5e-05", "0.1", "123.456", "1e+22", "-0.0", "5e-324", "1.0", "0.0001", "1.2345678901234568e+16", "2.5"));
        eq("repr", List.of(Py.repr("a'b"), Py.repr("x\u0001\u2028\ud83d\ude00\u00a0\u00e9")),
            List.of("\"a'b\"", "'x\\x01\\u2028\ud83d\ude00\\xa0\u00e9'"));
        eq("kelvin sign is not k", Gates.isFallback("fallbac\u212a template"), false);
        eq("$ before U+2028", Gates.isGeneric("tbd\u2028"), false);
        eq("$ before a final newline", Gates.isGeneric("tbd\n"), true);
        eq("no unicode digits", Gates.figures("\u0663.5 s and 3.5 s"), List.of("3.5 s"));
        eq("no unicode spaces", Gates.squash("a\u00a0 b\u3000"), "a\u00a0 b\u3000");
        eq("redact", Gates.redact("Token: abc  BEARER abcdefghij"), "Token: ***  BEARER ***");
        eq("lone surrogates around a dropped control character stay apart",
            Schema.checkView(Py.obj("\ud83d\u0001\ude00x", 1L, "a\ud800", 2L)), List.of("unknown keys (2): a\ufffd, \ufffd\ufffdx", "missing keys: contribution, devils_advocate, done_so_far, missing, sources, w5h1"));
        eq("exception text", firstViewError(new Flaky(new IllegalStateException("boom api_key=hunter2"))),
            "IllegalStateException: boom api_key=***");
        eq("exception without message", firstViewError(new Flaky(new UnsupportedOperationException())), "UnsupportedOperationException: ");
        eq("lone surrogate in a message", firstViewError(new Flaky(new AdapterError("bad \ud800 text"))), "AdapterError: bad ? text");
        eq("not plain data", firstViewError(new Flaky(Py.obj("w5h1", new Date(0)))), "invalid: Date is not JSON data");
        eq("not an object", firstViewError(new Flaky(List.of())), "invalid: a view must be a JSON object");
        throwsIae("lone surrogate in the idea", () -> new Council("idea \udc00", new StubAdapter()),
            "the idea, roster, phases and sources must not contain unpaired surrogates");
        throwsIae("duplicate roster", () -> new Council("idea", new StubAdapter(), List.of("A", "A"), null, null, 1, 1),
            "roster names must be unique non-empty strings");
        throwsIae("duplicate step", () -> new Council("idea", new StubAdapter(), null,
            List.of(List.of("a", "A", "?"), List.of("a", "B", "?")), null, 1, 1), "duplicate step_id a");
        throwsIae("max_loops", () -> new Council("idea", new StubAdapter(), null, null, null, 0, 1), "max_loops is an integer from 1 to 10");

        // a recorded run replays to the same rounds; its JSON text reads back as the same value
        Map<String, Object> rec = new Council("Replay me: 30% faster builds", new StubAdapter()).run();
        eq("council file is valid", Schema.checkCouncil(rec), List.of());
        Object back = Json.parse(Py.jsonDumps(rec, false));
        eq("json round trip", back, rec);
        Map<String, Object> again = new Council("Replay me: 30% faster builds", new ReplayAdapter(Py.map(back))).run();
        eq("replay rounds", again.get("rounds"), rec.get("rounds"));
        eq("replay adapter name", again.get("adapter"), "replay");
        eq("replay synthetic", again.get("synthetic"), true);
        eq("audit of a stub run", Audit.auditResult(rec).get("verdict"), "not build-ready");

        // a replay of a run with a failed view fails the same view
        Map<String, Object> bad = new Council("idea", new Flaky(new AdapterError("down")), List.of("Dev"),
            List.of(List.of("frame", "Frame", "Q?")), null, 1, 0).run();
        Map<String, Object> rebad = new Council("idea", new ReplayAdapter(bad), List.of("Dev"),
            List.of(List.of("frame", "Frame", "Q?")), null, 1, 0).run();
        eq("replayed failure", Py.get(Py.l(Py.l(rebad, "rounds").get(0), "views").get(0), "error"),
            "ReplayError: no recorded valid view for Dev at frame");
        eq("replayed failure health", Council.health(rebad).get("merges"), Py.obj("no_views", 1L));

        List<String> lines = new ArrayList<>(List.of("{\"output\": \"x\"}\n", "\n", "not json\n"));
        eq("readJsonl", Audit.readJsonl(lines), java.util.Arrays.asList(Py.obj("output", "x"), null));

        System.out.println(failed == 0 ? "java: selftest passed" : "java: selftest FAILED (" + failed + ")");
        System.exit(failed == 0 ? 0 : 1);
    }
}
