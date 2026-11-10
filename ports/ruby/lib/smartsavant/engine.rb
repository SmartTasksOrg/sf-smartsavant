# frozen_string_literal: true

require 'digest'

# The data model, the adapter interface with the stub, scripted and replay adapters, and the council engine:
# roster -> phases -> stakeholder views -> juror merge -> decision tree.
#   res = SmartSavant::Council.new('...', SmartSavant::StubAdapter.new).run   # a Hash: the council.json document
# Every answer is validated (check_view / check_merge) and retried; one that still fails is recorded as
# failed with an error, never filled with invented text. Deterministic for a deterministic adapter.
module SmartSavant
  # The council records as Hashes, with keys in the order of the Python dataclasses (model.py).
  module Model
    VIEW_STATUSES = %w[ok failed offline].freeze
    SYNTHESIS_STATUSES = %w[full thin recovered failed offline no_views].freeze

    module_function

    def w5h1(f = nil) = Schema::W5H1_KEYS.to_h { |k| [k, Py.get(f, k, '')] }

    def view(stakeholder:, role:, status: 'ok', w5h1: nil, done_so_far: '', missing: [], devils_advocate: '',
             contribution: '', sources: [], error: '')
      { 'stakeholder' => stakeholder, 'role' => role, 'status' => status, 'w5h1' => w5h1(w5h1),
        'done_so_far' => done_so_far, 'missing' => missing, 'devils_advocate' => devils_advocate,
        'contribution' => contribution, 'sources' => sources, 'error' => error }
    end

    def juror(synthesis_status: 'full', merge_summary: '', decision: '', resolved: [], open_issues: [], next_steps: [],
              reversals: [], spec_delta: '', loop_recommended: false, error: '')
      { 'synthesis_status' => synthesis_status, 'merge_summary' => merge_summary, 'decision' => decision,
        'resolved' => resolved, 'open_issues' => open_issues, 'next_steps' => next_steps, 'reversals' => reversals,
        'spec_delta' => spec_delta, 'loop_recommended' => loop_recommended, 'error' => error }
    end

    def round(step_id:, step_title:, iteration:, question:, views: [], juror: nil)
      { 'step_id' => step_id, 'step_title' => step_title, 'iteration' => iteration, 'question' => question,
        'views' => views, 'juror' => juror || self.juror }
    end

    def node(step_id:, iteration:, branches: [], decision: '', next_steps: [])
      { 'step_id' => step_id, 'iteration' => iteration, 'branches' => branches, 'decision' => decision,
        'next_steps' => next_steps }
    end

    def result(idea: '', adapter: '', synthetic: false, stakeholders: [], sources: [], rounds: [], decision_tree: [],
               locked_decisions: [], flags: [])
      { 'schema' => 'smartsavant.council/1', 'idea' => idea, 'adapter' => adapter, 'synthetic' => synthetic,
        'stakeholders' => stakeholders, 'sources' => sources, 'rounds' => rounds, 'decision_tree' => decision_tree,
        'locked_decisions' => locked_decisions, 'flags' => flags }
    end

    # A council.json document (check it with Schema.check_council first) as a fresh copy.
    def result_from_dict(d) = Py.copy(d)
  end

  # The one boundary between the engine and a model. An adapter answers two calls and nothing else:
  #   view(stakeholder:, role:, phase_id:, question:, context:)  -> a ViewAnswer Hash
  #   merge(phase_id:, question:, views:, context:)               -> a MergeAnswer Hash
  # (spec/schema/*.json, bindings/council.d.ts). context is a fresh copy for every call: {idea, iteration,
  # sources, prior, locked}. views are the valid view records of the phase. Set synthetic to true for
  # adapters whose output is not a real deliberation (stubs, demos).
  class LLMAdapter
    def name = 'adapter'
    def synthetic = false

    def view(stakeholder:, role:, phase_id:, question:, context:)
      raise NotImplementedError, 'view() is not implemented'
    end

    def merge(phase_id:, question:, views:, context:)
      raise NotImplementedError, 'merge() is not implemented'
    end
  end

  # A deterministic adapter: no model, no network. Its tag is "sv" + the first 8 hex digits of
  # sha256(stakeholder|phase_id|idea); "why" is the first 120 code points of the idea. Synthetic.
  class StubAdapter < LLMAdapter
    def name = 'stub'
    def synthetic = true

    def self.tag(*parts) = Digest::SHA256.hexdigest(parts.map(&:b).join('|'.b))[0, 8]

    def view(stakeholder:, role:, phase_id:, question:, context:)
      idea = Py.str(Py.get(context, 'idea', ''))
      t = StubAdapter.tag(stakeholder, phase_id, idea)
      {
        'w5h1' => { 'who' => stakeholder, 'what' => "advance #{phase_id}", 'where' => 'the target architecture',
                    'when' => 'this iteration', 'why' => Py.head(idea, 120), 'how' => "deterministic stub sv#{t}" },
        'done_so_far' => "#{role} framed #{phase_id}.",
        'missing' => ["#{phase_id}: target files and modules"],
        'devils_advocate' => "Risk at #{phase_id}: an unproven assumption (sv#{t}).",
        'contribution' => "#{role} proposes the #{phase_id} step sv#{t}.",
        'sources' => [],
      }
    end

    def merge(phase_id:, question:, views:, context:)
      resolved = views.each_with_index.map { |v, i| "#{phase_id} #{i + 1}: #{Py.str(v['contribution'])}" }.first(5)
      issues = views.flat_map { |v| Py.iter(Py.get(v, 'missing', [])) }.uniq.first(5)
      {
        'merge_summary' => "Stub juror merged #{views.length} view(s) for #{phase_id}.",
        'decision' => "#{phase_id}: proceed with the stub plan",
        'resolved' => resolved,
        'open_issues' => issues,
        'next_steps' => ["Write the #{phase_id} module for the stub plan"],
        'reversals' => [],
        'spec_delta' => "[#{phase_id}] stub decision recorded.",
        'loop_recommended' => false,
      }
    end
  end

  class AdapterError < RuntimeError; end
  # Python's KeyError, raised where the reference would raise it on a malformed script.
  class KeyError < StandardError; end

  # Answers from a script: {"views" => {"frame|1|Developer" => [ANSWER, ...]}, "merges" => {"build|1" => [...]}}.
  # One entry per call, the last one repeats; {"raise" => MESSAGE} raises AdapterError(MESSAGE); keys not in
  # the script get stub answers. Synthetic.
  class ScriptedAdapter < StubAdapter
    def name = 'scripted'
    def synthetic = true

    def initialize(script = nil)
      super()
      s = Py.truthy(script) ? script : {}
      @views = Py.copy(Py.get(s, 'views', {}))
      @merges = Py.copy(Py.get(s, 'merges', {}))
      @n = {}
    end

    def view(stakeholder:, role:, phase_id:, question:, context:)
      key = "#{phase_id}|#{Py.str(Py.get(context, 'iteration', 1))}|#{stakeholder}"
      return next_answer(@views, key) if @views.key?(key)
      super
    end

    def merge(phase_id:, question:, views:, context:)
      key = "#{phase_id}|#{Py.str(Py.get(context, 'iteration', 1))}"
      return next_answer(@merges, key) if @merges.key?(key)
      super
    end

    private

    def next_answer(table, key)
      seq = table[key]
      return nil unless Py.truthy(seq)
      i = @n.fetch(key, 0)
      @n[key] = i + 1
      seq = Py.chars(seq) if seq.is_a?(String)
      raise KeyError, [i, seq.length - 1].min.to_s if seq.is_a?(Hash)
      raise TypeError, "'#{seq.class}' object is not subscriptable" unless seq.is_a?(Array)
      ans = seq[[i, seq.length - 1].min]
      raise AdapterError, Py.str(ans['raise']) if ans.is_a?(Hash) && ans.keys == ['raise']
      Py.copy(ans)
    end
  end

  class ReplayError < RuntimeError; end

  # Replays the answers of a recorded run (a council.json written by an engine of this package). Views and
  # merges whose status was not ok / full / thin are replayed as failures (ReplayError).
  class ReplayAdapter < LLMAdapter
    attr_reader :synthetic

    def name = 'replay'

    # recorded: a council Hash, or the path of a council.json file.
    def initialize(recorded)
      super()
      doc = recorded.is_a?(String) ? Json.decode(File.binread(recorded)) : recorded
      @synthetic = Py.truthy(Py.get(doc, 'synthetic', false))
      @views = {}
      @merges = {}
      Py.get(doc, 'rounds', []).each do |r|
        Py.get(r, 'views', []).each { |v| @views[[r['step_id'], r['iteration'], v['stakeholder']]] ||= v }
        @merges[[r['step_id'], r['iteration']]] = Py.get(r, 'juror', {})
      end
    end

    def view(stakeholder:, role:, phase_id:, question:, context:)
      v = @views[[phase_id, Py.get(context, 'iteration', 1), stakeholder]]
      raise ReplayError, "no recorded valid view for #{stakeholder} at #{phase_id}" unless v && v['status'] == 'ok'
      Py.copy(v.slice(*Schema::VIEW_KEYS))
    end

    def merge(phase_id:, question:, views:, context:)
      m = @merges[[phase_id, Py.get(context, 'iteration', 1)]]
      raise ReplayError, "no recorded valid merge for #{phase_id}" unless m && %w[full thin].include?(m['synthesis_status'])
      Py.copy(Schema::MERGE_KEYS.to_h { |k| [k, m[k]] })
    end
  end

  class Council
    DEFAULT_ROSTER = ['Developer', 'UX/UI', 'Product Owner', 'Architecture/Infra'].freeze
    DEFAULT_PHASES = [
      ['frame', 'Frame', 'What is the problem, who are the users, and the jobs-to-be-done?'],
      ['concept', 'Concept', 'What is the solution direction and the wedge?'],
      ['design', 'Design', 'What are the user flow, data flow, and experience?'],
      ['build', 'Build', 'What is the technical approach?'],
      ['integrate', 'Integrate', 'How does it fit the target architecture?'],
      ['measure', 'Measure', 'What are the KPIs and success metrics?'],
      ['spec', 'Spec', 'Assemble the final document.'],
    ].freeze
    NOT_PRINTABLE = '(message not printable)'

    attr_reader :idea, :adapter, :roster, :phases, :sources, :max_loops, :retries

    # roster: stakeholder names (nil or [] for the default); phases: [[step_id, title, question], ...] or
    # [{"step_id", "title", "question"}, ...] (nil or [] for the default); sources: the ids views may cite.
    def initialize(idea, adapter, roster: nil, phases: nil, sources: nil, max_loops: 1, retries: 1)
      raise ArgumentError, 'the idea must be a non-empty string' if !idea.is_a?(String) || Gates.blank(idea)
      texts = [idea, *(roster || []), *(sources || []), *(phases || []).flat_map { |p| p.is_a?(Array) ? p : [] }]
      if texts.any? { |t| t.is_a?(String) && Py.lone?(t) }
        raise ArgumentError, 'the idea, roster, phases and sources must not contain unpaired surrogates'
      end
      raise TypeError, 'adapter must be an LLMAdapter' unless adapter.is_a?(LLMAdapter)
      @roster = roster && !roster.empty? ? roster.dup : DEFAULT_ROSTER.dup
      unless @roster.uniq.length == @roster.length && @roster.all? { |r| r.is_a?(String) && !Gates.blank(r) }
        raise ArgumentError, 'roster names must be unique non-empty strings'
      end
      @phases = phases && !phases.empty? ? Council.check_phases(phases) : DEFAULT_PHASES.map(&:dup)
      raise ArgumentError, 'max_loops is an integer from 1 to 10' unless max_loops.is_a?(Integer) && max_loops.between?(1, 10)
      raise ArgumentError, 'retries is an integer from 0 to 5' unless retries.is_a?(Integer) && retries.between?(0, 5)
      @idea = idea
      @adapter = adapter
      @sources = (sources || []).dup
      @max_loops = max_loops
      @retries = retries
    end

    # [[step_id, title, question]] with unique step ids and non-blank strings; a Hash phase is converted.
    def self.check_phases(phases)
      seen = {}
      out = phases.map do |p|
        p = [p['step_id'], p['title'], p['question']] if p.is_a?(Hash)
        unless p.is_a?(Array) && p.length == 3 && p.all? { |x| x.is_a?(String) && !Gates.blank(x) }
          raise ArgumentError, 'each phase is (step_id, title, question), three non-blank strings'
        end
        raise ArgumentError, "duplicate step_id #{p[0]}" if seen.key?(p[0])
        seen[p[0]] = true
        p.dup
      end
      raise ArgumentError, 'at least one phase' if out.empty?
      out
    end

    # A phases document ({"phases" => [{step_id, title, question}, ...]}, a Hash or a file path).
    def self.load_phases(source = nil)
      return DEFAULT_PHASES.map(&:dup) if source.nil?
      doc = source.is_a?(String) ? Json.decode(File.binread(source)) : source
      ph = Py.get(doc, 'phases')
      raise ArgumentError, 'a phases file is {"phases": [{"step_id", "title", "question"}, ...]}' unless ph.is_a?(Array) && !ph.empty?
      raise ArgumentError, 'each phase is an object {"step_id", "title", "question"}' unless ph.all? { |p| p.is_a?(Hash) }
      check_phases(ph)
    end

    # "<ExceptionName>: <message>": lone surrogates as '?', secrets redacted, at most 500 code points.
    def self.error_text(e)
      name = e.class.name.to_s.split('::').last || 'Exception'
      msg = begin
        m = e.message
        m.is_a?(String) ? Py.scrub(m) : NOT_PRINTABLE
      rescue StandardError
        NOT_PRINTABLE
      end
      Py.head(Gates.redact("#{name}: #{msg}"), 500)
    end

    # The council.json document as a Hash.
    def run
      a = @adapter
      rounds = []
      tree = []
      locked = []
      flags = []
      prior = []
      @phases.each do |step_id, title, question|
        it = 0
        loop do
          it += 1
          ctx = { 'idea' => @idea, 'iteration' => it, 'sources' => @sources, 'prior' => prior, 'locked' => locked }
          views = []
          @roster.each do |role|
            got, err = ask(->(c) { a.view(stakeholder: role, role: role, phase_id: step_id, question: question, context: c[:ctx]) },
                           ctx, nil, Schema.method(:check_view))
            if got.nil?
              views << Model.view(stakeholder: role, role: role, status: 'failed', error: err)
              next
            end
            raw = Schema.normalize_view(got)
            views << Model.view(stakeholder: role, role: role, status: 'ok', w5h1: raw['w5h1'], done_so_far: raw['done_so_far'],
                                missing: raw['missing'], devils_advocate: raw['devils_advocate'],
                                contribution: raw['contribution'], sources: raw['sources'])
            Gates.flag_unsourced(raw.merge('stakeholder' => role), @sources).each do |fl|
              flags << fl.merge('step_id' => step_id, 'iteration' => it)
            end
          end
          ok_views = views.select { |v| v['status'] == 'ok' }
          juror = if ok_views.empty?
                    Model.juror(synthesis_status: 'no_views', error: 'no valid view to merge')
                  else
                    m, err = ask(->(c) { a.merge(phase_id: step_id, question: question, views: c[:views], context: c[:ctx]) },
                                 ctx, ok_views, Schema.method(:check_merge))
                    if m.nil?
                      Model.juror(synthesis_status: 'failed', error: err)
                    else
                      j = Model.juror(synthesis_status: 'full', merge_summary: m['merge_summary'], decision: m['decision'],
                                      resolved: m['resolved'], open_issues: m['open_issues'], next_steps: m['next_steps'],
                                      reversals: m.fetch('reversals', []), spec_delta: m['spec_delta'],
                                      loop_recommended: m['loop_recommended'])
                      j['synthesis_status'] = 'thin' unless Gates.synthesis_ok(j)
                      j
                    end
                  end
          rounds << Model.round(step_id: step_id, step_title: title, iteration: it, question: question, views: views,
                                juror: juror)
          tree << Model.node(step_id: step_id, iteration: it, branches: ok_views.map { |v| v['contribution'] },
                             decision: juror['decision'], next_steps: juror['next_steps'].dup)
          if juror['synthesis_status'] == 'full'
            juror['resolved'].each { |r| locked << r unless locked.include?(r) }
            prior += [{ 'step' => step_id, 'decision' => juror['decision'] }]
          end
          break if !juror['loop_recommended'] || it >= @max_loops
        end
      end
      Model.result(idea: @idea, adapter: Py.str(a.name), synthetic: Py.truthy(a.synthetic), stakeholders: @roster.dup,
                   sources: @sources.dup, rounds: rounds, decision_tree: tree, locked_decisions: locked, flags: flags)
    end

    # Counts for a run: views by status, merges by synthesis status, flags, and whether every phase's last
    # iteration reached a full synthesis (decided).
    def self.health(res)
      views = Hash.new(0)
      merges = Hash.new(0)
      last = {}
      res['rounds'].each do |r|
        r['views'].each { |v| views[v['status']] += 1 }
        merges[r['juror']['synthesis_status']] += 1
        last[r['step_id']] = r['juror']['synthesis_status']
      end
      st = last.values
      { 'views' => Py.sorted(views.keys).to_h { |k| [k, views[k]] },
        'merges' => Py.sorted(merges.keys).to_h { |k| [k, merges[k]] },
        'phases' => last.length, 'phases_decided' => st.count('full'),
        'decided' => !st.empty? && st.all?('full'), 'flags' => res['flags'].length, 'synthetic' => res['synthetic'] }
    end

    private

    # [answer, ''] or [nil, error] after 1 + retries attempts.
    def ask(call, ctx, views, check)
      err = ''
      (1 + @retries).times do
        begin
          ans = Schema.plain(call.call({ ctx: Py.copy(ctx), views: Py.copy(views) }))   # validate exactly what will be used
        rescue NotPlainData => e
          err = Py.head("invalid: #{e.message}", 500)
          next
        rescue StandardError, NotImplementedError, SystemStackError => e               # an adapter failure is data, not a crash
          err = Council.error_text(e)
          next
        end
        problems = check.call(ans)
        return [ans, ''] if problems.empty?
        err = Py.head("invalid: #{problems.first(5).map { |p| Py.head(p, 200) }.join('; ')}", 500)
      end
      [nil, err]
    end
  end
end
