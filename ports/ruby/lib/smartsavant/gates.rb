# frozen_string_literal: true

require 'digest'

# Canonical JSON and the quality gates: pure functions over JSON data (vectors: spec/conformance/
# canonical.json, gates.json, figures.json). Every pattern is a binary (/n) pattern matched against the
# UTF-8 bytes, so digits, letters and case folding are ASCII only, as in Python's re.ASCII; whitespace is
# spelled [\x09-\x0D\x20] ([ \t\n\r\f\v]); non-ASCII characters are written as their UTF-8 bytes; Python's
# ^ and $ are \A and \Z.
module SmartSavant
  # Canonical JSON: keys ASCII and sorted, no whitespace, integers only, only " \ and controls escaped.
  module Canonical
    class Error < StandardError; end

    MAP = { '"' => '\\"', '\\' => '\\\\', "\b" => '\\b', "\f" => '\\f', "\n" => '\\n', "\r" => '\\r', "\t" => '\\t' }.freeze

    def self.str(s)
      raise Error, 'canonical JSON strings must be UTF-8 without unpaired surrogates' unless s.valid_encoding?
      "\"#{Py.utf8(s.b.gsub(/[\x00-\x1F"\\]/n) { |c| MAP[c] || format('\\u%04x', c.ord) })}\""
    end

    # The canonical JSON text (UTF-8). Floats, non-ASCII keys and other types raise Canonical::Error.
    def self.dumps(v)
      case v
      when nil then 'null'
      when true then 'true'
      when false then 'false'
      when Integer then v.to_s
      when String then str(v)
      when Array then "[#{v.map { |x| dumps(x) }.join(',')}]"
      when Hash
        v.each_key do |k|
          raise Error, 'canonical JSON object keys must be ASCII strings' unless k.is_a?(String) && k.b.ascii_only?
        end
        "{#{v.keys.sort_by(&:b).map { |k| "#{str(k)}:#{dumps(v[k])}" }.join(',')}}"
      else raise Error, "canonical JSON does not encode #{v.class} (floats are not allowed)"
      end
    end

    def self.sha256_hex(text) = Digest::SHA256.hexdigest(text.b)
  end

  module Gates
    S = '[\x09-\x0D\x20]'
    W = '[A-Za-z0-9_]'
    UNIT = '%|percent|ms|\xC2\xB5s|us|ns|s|sec|secs|seconds?|min|mins|minutes?|h|hrs?|hours?|days?|weeks?|' \
           'months?|years?|x|k|K|m|M|b|B|bn|mn|GB|MB|KB|TB|GiB|MiB|KiB|TiB|kpps|pps|rps|qps|tps|req/s|tokens?|users?|' \
           'agents?|USD|EUR|GBP'
    N = Regexp::NOENCODING
    I = Regexp::IGNORECASE | Regexp::NOENCODING
    FIG = Regexp.new("(?<![A-Za-z0-9_.])(?:(?<cur>\\xC2\\xA3|\\$|\\xE2\\x82\\xAC)#{S}?)?" \
                     '(?<num>[0-9]{1,3}(?:,[0-9]{3})+(?:\.[0-9]+)?|[0-9]+\.[0-9]+|[0-9]+)' \
                     "(?:#{S}?(?<unit>#{UNIT}))?(?!#{W})", N)
    GENERIC = [
      Regexp.new("\\A#{S}*advance the [A-Za-z0-9_/ -]+ with the decided approach[.!?]*#{S}*\\Z", I),
      Regexp.new("\\A#{S}*[A-Za-z]+#{S}+[XYZ][.!?]*#{S}*\\Z", N),
      Regexp.new("\\A#{S}*(next steps?#{S}*:?#{S}*)?(tbd|todo|n/?a|none|proceed|continue|next steps?)?[.!?]*#{S}*\\Z", I),
    ].freeze
    FALLBACK = Regexp.new('synthesis failed|(falling|fell|fall) back to the raw panel|returned unparseable output|' \
                          'unparseable output|offline fallback|\(offline\)|fallback template', I)
    PLACEHOLDER = Regexp.new("#{'#'}#{S}*TODO(?!#{W})|//#{S}*TODO(?!#{W})|return#{S}*\\{#{S}*['\"]ok['\"]#{S}*:#{S}*True#{S}*\\}|" \
                             "raise NotImplementedError|(?<!#{W})pass#{S}*#{'#'}#{S}*TODO", I)
    SECRET = Regexp.new("(?<!#{W})(api[_-]?key|access[_-]?token|token|secret|password|passwd|signature|sig|key)" \
                        "(#{S}*[=:]#{S}*)([^\\x09-\\x0D\\x20&;,\"']+)", I)
    BEARER = Regexp.new("(?<!#{W})(bearer|basic)#{S}+[A-Za-z0-9._~+/=-]{8,}", I)
    BLANK = /\A[\x09-\x0D\x20]*\z/n

    DEFAULT_RULES = [
      ['possible trading guidance on non-public information',
       [['non-public', 'nonpublic', 'mnpi', 'insider information'], %w[buy sell trade short position]]],
      ['possible credential or secret in the output',
       [['-----begin', 'aws_secret_access_key', 'api_key=', 'password=', 'ghp_', "github#{'_pat_'}", 'pypi-ag']]],
    ].freeze

    module_function

    def text(t) = Py.truthy(t) ? Py.str(t) : ''

    # Runs of [ \t\n\r\f\v] to one space, trimmed.
    def squash(t) = Py.utf8(text(t).b.split(/[\x09-\x0D\x20]+/n).reject(&:empty?).join(' '))

    # True for a non-string, or a string empty or made only of [ \t\n\r\f\v].
    def blank(t) = !t.is_a?(String) || BLANK.match?(t.b)

    # Figures in order of appearance, as written.
    def figures(t)
      out = []
      text(t).b.scan(FIG) do
        m = Regexp.last_match
        out << Py.utf8(m[0].strip) if m[:cur] || m[:unit] || m[:num].include?(',') || m[:num].include?('.')
      end
      out
    end

    def view_text(view)
      missing = Py.get(view, 'missing', '')
      missing = missing.map { |x| Py.str(x) }.join(' ') if missing.is_a?(Array)
      [Py.get(view, 'contribution', ''), Py.get(view, 'devils_advocate', ''), Py.get(view, 'done_so_far', ''), missing]
        .map { |x| Py.str(x) }.join(' ')
    end

    # unsourced_figures flags when the view cites no source, then unknown_source per cited id not allowed.
    def flag_unsourced(view, allowed_sources)
      out = []
      who = Py.get(view, 'stakeholder', '')
      src = Py.get(view, 'sources', [])
      cited = (Py.truthy(src) ? Py.iter(src) : []).select { |s| s.is_a?(String) }
      if cited.empty?
        figures(view_text(view)).each { |f| out << { 'flag' => 'unsourced_figures', 'figure' => f, 'stakeholder' => who } }
      end
      allowed = Py.truthy(allowed_sources) ? Py.iter(allowed_sources) : []
      cited.each { |s| out << { 'flag' => 'unknown_source', 'source' => s, 'stakeholder' => who } unless allowed.include?(s) }
      out
    end

    # A next step or story title that names no real component or decision (fallback text included).
    def generic?(step) = blank(step) || GENERIC.any? { |p| p.match?(step.b) } || fallback?(step)

    def fallback?(t) = FALLBACK.match?(text(t).b)

    def placeholder_code?(t) = PLACEHOLDER.match?(text(t).b)

    # A merge that genuinely decided something.
    def synthesis_ok(merge)
      return false unless Py.get(merge, 'synthesis_status', 'full') == 'full'
      summary = Py.get(merge, 'merge_summary', '')
      decision = Py.get(merge, 'decision', '')
      return false if blank(summary) || blank(decision)
      resolved = Py.get(merge, 'resolved')
      steps = Py.get(merge, 'next_steps')
      rs = Py.truthy(resolved) ? Py.iter(resolved) : []
      ss = Py.truthy(steps) ? Py.iter(steps) : []
      return false if [summary, decision, *rs, *ss].any? { |t| fallback?(t) }
      !rs.empty? && ss.any? { |s| !generic?(s) }
    end

    # Reasons to block a release of this text ([] when no rule fired). Keyword matching only.
    # rules: [[name, [[word, ...], ...]], ...]; nil means the defaults.
    def compliance_block(t, rules = nil)
      low = Py.lower(text(t))
      (rules || DEFAULT_RULES).filter_map { |name, groups| name if groups.all? { |g| g.any? { |w| low.include?(w) } } }
    end

    # Error text made safe to record: key=VALUE-style secrets and bearer tokens replaced by ***.
    def redact(t)
      b = text(t).b.gsub(SECRET) { "#{Regexp.last_match(1)}#{Regexp.last_match(2)}***" }
      Py.utf8(b.gsub(BEARER) { "#{Regexp.last_match(1)} ***" })
    end
  end
end
