# frozen_string_literal: true

# The importer (SmartSignal council exports -> a council result) and the audit of a council run, its stories
# or its trainset. Codes and severities as in src/sf_smartsavant/audit.py:
#   AUDIT-NO-SYNTHESIS error, AUDIT-OFFLINE-VIEWS error (warning when fewer than half), AUDIT-FAILED-VIEWS
#   warning, AUDIT-FALLBACK-TEXT error, AUDIT-TEMPLATE-VIEWS error, AUDIT-GENERIC-STEPS warning (error when
#   all are), AUDIT-PLACEHOLDER-CODE warning, AUDIT-UNSOURCED warning, AUDIT-COMPLIANCE error,
#   AUDIT-SYNTHETIC error, AUDIT-TRAINSET-CONTAMINATED error, AUDIT-TRAINSET-UNREADABLE error.
module SmartSavant
  module Importer
    STATUS = %w[full recovered offline failed thin no_views].freeze
    MAX_DEPTH = 20

    module_function

    # Unpaired surrogates (which cannot be written as UTF-8) become U+FFFD.
    def clean(t) = Py.replace_lone(t, "�")

    def text(v, depth = 0)
      return '[nested too deep]' if depth > MAX_DEPTH
      case v
      when nil then ''
      when String then clean(v)
      when Hash then v.map { |k, x| "#{text(k, depth + 1)}: #{text(x, depth + 1)}" }.join("\n")
      when Array then v.map { |x| text(x, depth + 1) }.join('; ')
      when true then 'true'
      when false then 'false'
      when Integer then v.to_s
      when Float then v.finite? ? Py.float_repr(v) : 'float'
      else v.class.name.to_s
      end
    end

    def dict(v) = v.is_a?(Hash) ? v : {}
    def list(v) = v.is_a?(Array) ? v : []

    def strs(v)
      case v
      when nil then []
      when String then Gates.blank(v) ? [] : [v]
      when Array then v.map { |x| text(x) }.reject { |x| Gates.blank(x) }
      else [text(v)]
      end
    end

    def idea_text(idea)
      return idea if idea.is_a?(String)
      if idea.is_a?(Hash)
        name, prob, sol, aud = %w[name problem solution audience].map { |k| text(Py.get(idea, k, '')) }
        s = "#{name}: #{prob}".b.sub(/\A[: ]+/n, '').sub(/[: ]+\z/n, '')
        s = Py.utf8(s)
        s += " -> #{sol}" unless sol.empty?
        s += " (audience: #{aud})" unless aud.empty?
        return s
      end
      text(idea)
    end

    # 'smartsavant', 'smartsignal-rounds', 'smartsignal-result' or nil.
    def detect(doc)
      return nil unless doc.is_a?(Hash)
      return 'smartsavant' if doc['schema'] == 'smartsavant.council/1'
      rounds = doc['rounds']
      return nil unless rounds.is_a?(Array) && !rounds.empty? && rounds.any? { |r| r.is_a?(Hash) && r.key?('views') }
      rounds.each do |r|
        list(dict(r)['views']).each do |v|
          return v.key?('parsed') ? 'smartsignal-rounds' : 'smartsignal-result' if v.is_a?(Hash)
        end
      end
      'smartsignal-result'
    end

    def small_int?(x) = x.is_a?(Integer) && x >= 0 && x < 1000

    # A SmartSignal council export as a council result (a Hash like Council#run returns).
    def from_smartsignal(doc)
      kind = detect(doc)
      raise ArgumentError, 'not a SmartSignal council export (no rounds with views)' unless %w[smartsignal-rounds smartsignal-result].include?(kind)
      models = []
      seen = Hash.new(0)
      names = []
      rounds = []
      tree = []
      flags = []
      locked = []
      doc['rounds'].each do |r|
        next unless r.is_a?(Hash)
        step = text(Py.get(r, 'step_id', 'step'))
        step = 'step' if step.empty?
        iter = r['iteration']
        rnd = r['round']
        it = small_int?(iter) ? iter + 1 : small_int?(rnd) ? rnd + 1 : 1
        key = [step, it]
        seen[key] += 1
        step = "#{step}.#{seen[key]}" if seen[key] > 1
        views = []
        list(r['views']).each do |v|
          next unless v.is_a?(Hash)
          p = dict(kind == 'smartsignal-rounds' ? Py.get(v, 'parsed', v) : v)
          src = text(Py.get(v, 'source', ''))
          models << text(v['model']) if Py.truthy(v['model'])
          w = dict(p['w5h1'])
          who = text(Py.get(v, 'stakeholder', ''))
          names << who unless names.include?(who)
          views << Model.view(stakeholder: who, role: text(Py.get(v, 'role', '')), status: src == 'offline' ? 'offline' : 'ok',
                              w5h1: Schema::W5H1_KEYS.to_h { |k| [k, text(Py.get(w, k, ''))] },
                              done_so_far: text(p['done_so_far']), missing: strs(p['missing']),
                              devils_advocate: text(p['devils_advocate']), contribution: text(p['contribution']),
                              sources: strs(p['sources']).select { |s| Py.len(s) <= 200 })
        end
        j = dict(r['juror'])
        models << text(j['model']) if Py.truthy(j['model'])
        st = text(Py.get(j, 'synthesis_status', ''))
        st = 'failed' if st.empty? || !STATUS.include?(st)
        resolved = strs(j['resolved'])
        decision = text(j['decision'])
        juror = Model.juror(synthesis_status: st, merge_summary: text(j['merge_summary']),
                            decision: decision.empty? ? (resolved[0] || '') : decision,
                            resolved: resolved, open_issues: strs(j['open_issues']), next_steps: strs(j['next_steps']),
                            reversals: strs(j['reversals']), spec_delta: text(j['spec_delta']),
                            loop_recommended: true.equal?(j['loop_recommended']))
        if st == 'full' && !Gates.synthesis_ok(juror)
          juror['synthesis_status'] = 'thin'
          juror['error'] = 'imported as full, but it fails the synthesis gate'
        end
        ok_views = views.select { |x| x['status'] == 'ok' }
        title = text(Py.get(r, 'step_title', step))
        rounds << Model.round(step_id: step, step_title: title.empty? ? step : title, iteration: it,
                              question: text(Py.get(r, 'question', Py.get(r, 'step_title', ''))), views: views, juror: juror)
        tree << Model.node(step_id: step, iteration: it, branches: ok_views.map { |x| x['contribution'] },
                           decision: juror['decision'], next_steps: juror['next_steps'].dup)
        ok_views.each do |x|
          fv = { 'stakeholder' => x['stakeholder'], 'contribution' => x['contribution'], 'devils_advocate' => x['devils_advocate'],
                 'done_so_far' => x['done_so_far'], 'missing' => x['missing'], 'sources' => x['sources'] }
          Gates.flag_unsourced(fv, []).each { |fl| flags << fl.merge('step_id' => step, 'iteration' => it) }
        end
        juror['resolved'].each { |d| locked << d unless locked.include?(d) } if juror['synthesis_status'] == 'full'
      end
      decs = doc['decisions']
      any_full = rounds.any? { |x| x['juror']['synthesis_status'] == 'full' }
      if any_full && decs.is_a?(Array) && !decs.empty? && decs.all? { |d| d.is_a?(Hash) }
        locked = decs.map { |d| "#{text(Py.get(d, 'topic', ''))}: #{text(Py.get(d, 'decision', ''))}" }.reject { |x| Gates.fallback?(x) }
      end
      sh = list(doc['stakeholders']).map { |s| s.is_a?(Hash) ? text(Py.get(s, 'name', '')) : text(s) }
      models = Py.sorted(models.uniq)
      Model.result(idea: clean(idea_text(Py.get(doc, 'idea', ''))),
                   adapter: models.empty? ? 'smartsignal' : "smartsignal:#{models.join(',')}",
                   synthetic: false, stakeholders: sh.empty? ? names : sh, sources: [], rounds: rounds, decision_tree: tree,
                   locked_decisions: locked, flags: flags)
    end
  end

  module Audit
    MAX_EXAMPLES = 5
    SYNTHETIC_ADAPTERS = %w[stub scripted].freeze
    ORDER = { 'error' => 0, 'warning' => 1, 'info' => 2 }.freeze

    module_function

    def finding(code, severity, count, detail, examples = [])
      { 'code' => code, 'severity' => severity, 'count' => count, 'detail' => detail,
        'examples' => examples.first(MAX_EXAMPLES).map { |e| Py.head(Py.str(e), 200) } }
    end

    def decided?(r) = r['juror']['synthesis_status'] == 'full' && Gates.synthesis_ok(r['juror'])

    def audit_result(res)
      h = Council.health(res)
      out = []
      rounds = res['rounds']
      last = Exporters.last_rounds(res)
      undecided = last.reject { |r| decided?(r) }.map do |r|
        st = r['juror']['synthesis_status']
        "#{r['step_id']}: #{st}#{st == 'full' ? ' (recorded, but it fails the synthesis gate)' : ''}"
      end
      unless undecided.empty?
        out << finding('AUDIT-NO-SYNTHESIS', 'error', undecided.length,
                       "#{undecided.length} of #{last.length} phases have no full synthesis", undecided)
      end
      pairs = rounds.flat_map { |r| r['views'].map { |v| [r, v] } }
      off = pairs.select { |_, v| v['status'] == 'offline' }.map { |r, v| "#{r['step_id']}/#{v['stakeholder']}" }
      unless off.empty?
        sev = 2 * off.length >= [1, pairs.length].max ? 'error' : 'warning'
        out << finding('AUDIT-OFFLINE-VIEWS', sev, off.length, "#{off.length} of #{pairs.length} views are offline templates", off)
      end
      failed = pairs.select { |_, v| v['status'] == 'failed' }.map { |r, v| "#{r['step_id']}/#{v['stakeholder']}: #{v['error']}" }
      out << finding('AUDIT-FAILED-VIEWS', 'warning', failed.length, "#{failed.length} views were never valid", failed) unless failed.empty?
      fb = []
      rounds.each do |r|
        j = r['juror']
        fields = [['merge_summary', j['merge_summary']], ['decision', j['decision']], ['spec_delta', j['spec_delta']]]
        %w[resolved next_steps open_issues].each { |k| j[k].each_with_index { |t, i| fields << ["#{k}[#{i}]", t] } }
        fields.each { |name, t| fb << "#{r['step_id']}.#{name}: #{Py.head(t, 80)}" if Gates.fallback?(t) }
      end
      out << finding('AUDIT-FALLBACK-TEXT', 'error', fb.length, 'fallback text where a decision should be', fb) unless fb.empty?
      # the same devil's-advocate or contribution text in several phases: a template
      by_text = {}
      rounds.each do |r|
        r['views'].each do |v|
          [v['devils_advocate'], v['contribution']].each do |t|
            sq = Gates.squash(t)
            next if Py.len(sq) < 20
            (by_text[[v['stakeholder'], sq.b]] ||= [v['stakeholder'], sq, {}])[2][r['step_id'].b.split('.'.b, 2)[0]] = true
          end
        end
      end
      tmpl = by_text.values.select { |_, _, steps| steps.length >= 3 }.map { |who, sq, _| "#{who}: #{Py.head(sq, 60)}" }
      out << finding('AUDIT-TEMPLATE-VIEWS', 'error', tmpl.length, 'the same view text appears in 3 or more phases', tmpl) unless tmpl.empty?
      steps = last.flat_map { |r| r['juror']['next_steps'] }
      gen = steps.select { |s| Gates.generic?(s) }
      unless gen.empty?
        sev = gen.length == steps.length ? 'error' : 'warning'
        out << finding('AUDIT-GENERIC-STEPS', sev, gen.length, "#{gen.length} of #{steps.length} next steps name nothing", gen)
      end
      ph = pairs.select { |_, v| Gates.placeholder_code?(v['contribution']) }.map { |r, v| "#{r['step_id']}/#{v['stakeholder']}" }
      ph += rounds.select { |r| Gates.placeholder_code?(r['juror']['spec_delta']) }.map { |r| "#{r['step_id']}.spec_delta" }
      out << finding('AUDIT-PLACEHOLDER-CODE', 'warning', ph.length, 'placeholder code presented as content', ph) unless ph.empty?
      unless res['flags'].empty?
        ex = res['flags'].map do |x|
          what = Py.get(x, 'figure')
          what = Py.get(x, 'source') unless Py.truthy(what)
          "#{Py.str(Py.get(x, 'step_id'))}/#{Py.str(Py.get(x, 'stakeholder'))}: #{Py.str(what)}"
        end
        out << finding('AUDIT-UNSOURCED', 'warning', res['flags'].length, 'figures without a source, or unknown sources', ex)
      end
      text = [res['idea']] + rounds.flat_map do |r|
        [r['juror']['merge_summary'], r['juror']['decision'], r['juror']['spec_delta'], *r['views'].map { |v| v['contribution'] }]
      end
      comp = Gates.compliance_block(Py.utf8(text.map(&:b).join(' '.b)))
      out << finding('AUDIT-COMPLIANCE', 'error', comp.length, 'compliance rules fired', comp) unless comp.empty?
      adapter = res['adapter']
      if res['synthetic'] || SYNTHETIC_ADAPTERS.include?(adapter) || adapter.start_with?('demo')
        out << finding('AUDIT-SYNTHETIC', 'error', 1, "adapter #{adapter} is synthetic: not a real deliberation")
      end
      report('council', h, out)
    end

    def audit_stories(doc)
      stories = doc.is_a?(Hash) ? Py.iter(Py.get(doc, 'stories', [])) : []
      titles = stories.select { |s| s.is_a?(Hash) }.map { |s| Py.get(s, 'title', '') }
      gen = titles.select { |t| Gates.generic?(t) }
      blocked = stories.select { |s| s.is_a?(Hash) && s['status'] == 'blocked' }.map { |s| Py.get(s, 'story_id', '') }
      out = []
      unless gen.empty?
        sev = gen.length == titles.length ? 'error' : 'warning'
        out << finding('AUDIT-GENERIC-STEPS', sev, gen.length, "#{gen.length} of #{titles.length} story titles name nothing", gen)
      end
      unless blocked.empty?
        out << finding('AUDIT-NO-SYNTHESIS', 'error', blocked.length, "#{blocked.length} stories are blocked (no decision)", blocked)
      end
      out << finding('AUDIT-NO-SYNTHESIS', 'error', 0, 'no stories') if titles.empty?
      report('stories', { 'stories' => titles.length }, out)
    end

    def row_text(row)
      msgs = row['messages']
      if msgs.is_a?(Array)
        return msgs.select { |m| m.is_a?(Hash) && m['role'] == 'assistant' }.map { |m| Py.str(Py.get(m, 'content', '')) }.join(' ')
      end
      Py.str(Py.get(row, 'output', ''))
    end

    # rows: trainset rows (nil for a line that was not JSON).
    def audit_trainset(rows)
      bad = []
      unreadable = []
      kinds = Hash.new(0)
      rows.each_with_index do |row, i|
        unless row.is_a?(Hash) && (row['output'].is_a?(String) || row['messages'].is_a?(Array))
          unreadable << "row #{i}: not an object with an output or messages"
          next
        end
        meta = Py.get(row, 'meta', {})
        k = meta.is_a?(Hash) ? meta['kind'] : nil
        k = 'unknown' unless k.is_a?(String)
        kinds[k] += 1
        t = row_text(row)
        why = []
        why << 'fallback' if Gates.fallback?(t)
        why << 'placeholder code' if Gates.placeholder_code?(t)
        bad << "row #{i} (#{k}): #{why.join(', ')}" unless why.empty?
      end
      out = []
      unless unreadable.empty?
        out << finding('AUDIT-TRAINSET-UNREADABLE', 'error', unreadable.length,
                       "#{unreadable.length} of #{rows.length} rows cannot be used", unreadable)
      end
      unless bad.empty?
        out << finding('AUDIT-TRAINSET-CONTAMINATED', 'error', bad.length,
                       "#{bad.length} of #{rows.length} rows teach fallback or placeholder output", bad)
      end
      out << finding('AUDIT-TRAINSET-CONTAMINATED', 'error', 0, 'no rows') if rows.empty?
      report('trainset', { 'rows' => rows.length, 'kinds' => Py.sorted(kinds.keys).map { |x| [x, kinds[x]] } }, out)
    end

    def report(kind, summary, findings)
      findings = findings.each_with_index.sort_by { |f, i| [ORDER[f['severity']], f['code'].b, i] }.map(&:first)
      errors = findings.count { |f| f['severity'] == 'error' }
      { 'kind' => kind, 'summary' => summary, 'findings' => findings, 'verdict' => errors.zero? ? 'build-ready' : 'not build-ready' }
    end

    MAX_ROWS = 200_000
    MAX_CHARS = 256 * 1024 * 1024
    MAX_LINE = 1024 * 1024
    MAX_LINES = 1_000_000

    # Parsed rows (nil for a line that is not JSON), stopping at the row, size and line budgets.
    def read_jsonl(lines)
      rows_left = MAX_ROWS
      chars = MAX_CHARS
      lines_left = MAX_LINES
      rows = []
      lines.each do |line|
        chars -= Py.len(line)
        lines_left -= 1
        break if chars.negative? || rows_left <= 0 || lines_left.negative?
        line = Py.utf8(line.b.sub(/\A[ \t\r\n]+/n, '').sub(/[ \t\r\n]+\z/n, ''))
        next if line.empty?
        rows_left -= 1
        if Py.len(line) > MAX_LINE
          rows << nil
          next
        end
        rows << begin
          Json.decode(line)
        rescue Json::Error
          nil
        end
      end
      rows
    end

    # Audit one file: a council JSON (an engine of this package, or SmartSignal), a stories JSON or a .jsonl
    # trainset. Zip archives are read by the Python reference only.
    def audit_path(path)
      raise ArgumentError, 'zip trainsets are read by the Python reference only' if path.end_with?('.zip')
      body = File.binread(path)
      if path.end_with?('.jsonl')
        text = Py.utf8(body).scrub("�").gsub(/\r\n?/, "\n")
        return audit_trainset(read_jsonl(text.each_line))
      end
      doc = Json.decode(body)
      doc = { 'rounds' => doc } if doc.is_a?(Array) && !doc.empty? && doc.all? { |r| r.is_a?(Hash) && r.key?('views') }
      kind = Importer.detect(doc)
      if kind == 'smartsavant'
        errs = Schema.check_council(doc)
        raise ArgumentError, "not a valid council file: #{errs.first(3).join('; ')}" unless errs.empty?
        return audit_result(Model.result_from_dict(doc))
      end
      return audit_result(Importer.from_smartsignal(doc)).merge('source' => kind) if kind
      return audit_stories(doc) if doc.is_a?(Hash) && doc['stories'].is_a?(Array)
      raise ArgumentError, 'not a council result, a council export, a stories file or a trainset'
    end
  end
end
