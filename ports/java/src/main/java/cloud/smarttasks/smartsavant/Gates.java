package cloud.smarttasks.smartsavant;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Quality gates: pure functions over plain JSON values (vectors: spec/conformance/gates.json, figures.json).
 *
 * <p>Every pattern has ASCII semantics as SEMANTICS.md section 2 requires: Java's {@code \s} is
 * {@code [ \t\n\x0B\f\r]}, {@code \w} and {@code \d} are ASCII without UNICODE_CHARACTER_CLASS,
 * CASE_INSENSITIVE without UNICODE_CASE folds ASCII letters only, the word boundary is spelled out with
 * {@code \w} lookarounds, and UNIX_LINES makes {@code $} accept only {@code \n} before the end, as in Python.
 */
public final class Gates {
    private Gates() {}

    private static final String B = "(?:(?<=\\w)(?!\\w)|(?<!\\w)(?=\\w))";
    private static final int I = Pattern.CASE_INSENSITIVE | Pattern.UNIX_LINES;

    private static final String UNIT = "%|percent|ms|\u00b5s|us|ns|s|sec|secs|seconds?|min|mins|minutes?|h|hrs?|hours?|"
        + "days?|weeks?|months?|years?|x|k|K|m|M|b|B|bn|mn|GB|MB|KB|TB|GiB|MiB|KiB|TiB|kpps|pps|rps|qps|tps|req/s|"
        + "tokens?|users?|agents?|USD|EUR|GBP";
    private static final Pattern FIG = Pattern.compile(
        "(?<![\\w.])"
            + "(?:(?<cur>[\u00a3$\u20ac])\\s?)?"
            + "(?<num>\\d{1,3}(?:,\\d{3})+(?:\\.\\d+)?|\\d+\\.\\d+|\\d+)"
            + "(?:\\s?(?<unit>" + UNIT + "))?"
            + "(?!\\w)",
        Pattern.UNIX_LINES);
    private static final Pattern WS = Pattern.compile("[ \\t\\n\\r\\f\\x0B]+");

    /** Runs of ASCII whitespace (space, tab, LF, CR, FF, VT) to one space, trimmed. */
    public static String squash(String text) {
        if (text == null) return "";
        StringBuilder b = new StringBuilder();
        for (String w : WS.split(text)) {
            if (w.isEmpty()) continue;
            if (b.length() > 0) b.append(' ');
            b.append(w);
        }
        return b.toString();
    }

    private static boolean ws(char c) { return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == 0x0B; }

    /** True for a non-string, or a string that is empty or made only of [ \t\n\r\f\v]. */
    public static boolean blank(Object text) {
        if (!(text instanceof String s)) return true;
        for (int k = 0; k < s.length(); k++) if (!ws(s.charAt(k))) return false;
        return true;
    }

    /** {@code text or ''} for the text gates: a string as is, a falsy value as "", anything else as str(). */
    private static String text(Object v) { return v instanceof String s ? s : Py.truthy(v) ? Py.str(v) : ""; }

    /** Figures in order of appearance, as written. */
    public static List<String> figures(Object text) {
        List<String> out = new ArrayList<>();
        Matcher m = FIG.matcher(text(text));
        while (m.find()) {
            String num = m.group("num");
            if (m.group("cur") != null || m.group("unit") != null || num.contains(",") || num.contains(".")) {
                out.add(m.group().strip());
            }
        }
        return out;
    }

    private static String viewText(Object view) {
        Object missing = Py.get(view, "missing", "");
        if (missing instanceof List<?> l) {
            StringBuilder b = new StringBuilder();
            for (int k = 0; k < l.size(); k++) b.append(k > 0 ? " " : "").append(Py.str(l.get(k)));
            missing = b.toString();
        }
        return String.join(" ", Py.str(Py.get(view, "contribution", "")), Py.str(Py.get(view, "devils_advocate", "")),
            Py.str(Py.get(view, "done_so_far", "")), Py.str(missing));
    }

    /**
     * Flags for one view: unsourced_figures for every figure when it cites no source, then unknown_source per
     * cited id not in allowedSources (when the run allows no sources, every cited id is unknown).
     */
    public static List<Map<String, Object>> flagUnsourced(Object view, Object allowedSources) {
        List<Map<String, Object>> out = new ArrayList<>();
        Object who = Py.get(view, "stakeholder", "");
        Object src = Py.get(view, "sources", List.of());
        List<String> cited = new ArrayList<>();
        if (Py.truthy(src)) for (Object s : Py.iter(src)) if (s instanceof String x) cited.add(x);
        if (cited.isEmpty()) {
            for (String f : figures(viewText(view))) out.add(Py.obj("flag", "unsourced_figures", "figure", f, "stakeholder", who));
        }
        Set<Object> allowed = new HashSet<>(Py.truthy(allowedSources) ? Py.iter(allowedSources) : List.of());
        for (String s : cited) if (!allowed.contains(s)) out.add(Py.obj("flag", "unknown_source", "source", s, "stakeholder", who));
        return out;
    }

    private static final Pattern[] GENERIC = {
        Pattern.compile("^\\s*advance the [\\w/ -]+ with the decided approach[.!?]*\\s*$", I),
        Pattern.compile("^\\s*[A-Za-z]+\\s+[XYZ][.!?]*\\s*$", Pattern.UNIX_LINES),
        Pattern.compile("^\\s*(next steps?\\s*:?\\s*)?(tbd|todo|n/?a|none|proceed|continue|next steps?)?[.!?]*\\s*$", I),
    };

    private static final Pattern FALLBACK = Pattern.compile("synthesis failed|(falling|fell|fall) back to the raw panel|"
        + "returned unparseable output|unparseable output|offline fallback|\\(offline\\)|fallback template", I);

    public static boolean isFallback(Object text) { return FALLBACK.matcher(text(text)).find(); }

    /** A next step or story title that names no real component or decision (fallback text included). */
    public static boolean isGeneric(Object step) {
        if (blank(step)) return true;
        for (Pattern p : GENERIC) if (p.matcher((String) step).find()) return true;
        return isFallback(step);
    }

    private static final Pattern PLACEHOLDER = Pattern.compile("#\\s*TODO" + B + "|//\\s*TODO" + B
        + "|return\\s*\\{\\s*['\"]ok['\"]\\s*:\\s*True\\s*\\}|raise NotImplementedError|" + B + "pass\\s*#\\s*TODO", I);

    public static boolean hasPlaceholderCode(Object text) { return PLACEHOLDER.matcher(text(text)).find(); }

    /** A merge that genuinely decided something. */
    public static boolean synthesisOk(Object merge) {
        if (!"full".equals(Py.get(merge, "synthesis_status", "full"))) return false;
        Object summary = Py.get(merge, "merge_summary", "");
        Object decision = Py.get(merge, "decision", "");
        if (blank(summary) || blank(decision)) return false;
        Object resolved = Py.get(merge, "resolved");
        Object steps = Py.get(merge, "next_steps");
        List<Object> rs = Py.truthy(resolved) ? Py.iter(resolved) : List.of();
        List<Object> ss = Py.truthy(steps) ? Py.iter(steps) : List.of();
        List<Object> all = new ArrayList<>(List.of(summary, decision));
        all.addAll(rs);
        all.addAll(ss);
        for (Object t : all) if (isFallback(t)) return false;
        if (!Py.truthy(resolved)) return false;
        for (Object s : ss) if (!isGeneric(s)) return true;
        return false;
    }

    /** A compliance rule: it fires when the lowercased text contains a word from every group. */
    public record Rule(String name, List<List<String>> groups) {}

    public static final List<Rule> DEFAULT_RULES = List.of(
        new Rule("possible trading guidance on non-public information", List.of(
            List.of("non-public", "nonpublic", "mnpi", "insider information"),
            List.of("buy", "sell", "trade", "short", "position"))),
        new Rule("possible credential or secret in the output", List.of(
            List.of("-----begin", "aws_secret_access_key", "api_key=", "password=", "ghp_", "github" + "_pat_", "pypi-ag"))));

    /** Reasons to block a release of this text (empty when no default rule fired). Keyword matching only. */
    public static List<String> complianceBlock(Object text) { return complianceBlock(text, null); }

    public static List<String> complianceBlock(Object text, List<Rule> rules) {
        String low = Py.lower(text(text));
        List<String> out = new ArrayList<>();
        for (Rule r : rules == null ? DEFAULT_RULES : rules) {
            boolean all = true;
            for (List<String> g : r.groups()) {
                boolean any = false;
                for (String w : g) if (low.contains(w)) { any = true; break; }
                if (!any) { all = false; break; }
            }
            if (all) out.add(r.name());
        }
        return out;
    }

    private static final Pattern SECRET = Pattern.compile(B + "(api[_-]?key|access[_-]?token|token|secret|password|passwd|"
        + "signature|sig|key)(\\s*[=:]\\s*)([^\\s&;,\"']+)", I);
    private static final Pattern BEARER = Pattern.compile(B + "(bearer|basic)\\s+[A-Za-z0-9._~+/=-]{8,}", I);

    /** Error text made safe to record: key=VALUE-style secrets and bearer tokens replaced by ***. */
    public static String redact(String text) {
        String t = text == null ? "" : text;
        t = SECRET.matcher(t).replaceAll(m -> Matcher.quoteReplacement(m.group(1) + m.group(2) + "***"));
        return BEARER.matcher(t).replaceAll(m -> Matcher.quoteReplacement(m.group(1) + " ***"));
    }
}
