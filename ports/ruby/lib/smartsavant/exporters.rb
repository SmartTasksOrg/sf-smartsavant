# frozen_string_literal: true

# Exporters: a council result -> user stories, a spec in Markdown, a fine-tune trainset, a Mermaid tree.
# Every exporter reports what the run did not decide instead of covering it up.
module SmartSavant
  module Exporters
    METHODOLOGY = {
      'frame' => 'Establish the problem, the real users and their jobs-to-be-done, and agree on one long-term goal.',
      'concept' => 'Collect solution directions, then pick the wedge: the smallest offer the users would switch for.',
      'design' => 'Draw the user flow and the data flow end to end, and name every screen and every store.',
      'build' => 'Decide the technical approach and the riskiest part to prove first.',
      'integrate' => 'Fit the design into the target architecture: owners, interfaces, auth, deployment.',
      'measure' => 'Set the KPIs, how each one is measured, and the thresholds that would change the plan.',
      'spec' => 'Assemble the decisions into one document that an engineer can build from.',
    }.freeze
    VIEW_FIELDS = %w[w5h1 done_so_far missing devils_advocate contribution sources].freeze
    # control and bidirectional-control characters (C0, DEL, C1, U+202A-U+202E, U+2066-U+2069)
    BAD = /[\x00-\x1F\x7F]|\xC2[\x80-\x9F]|\xE2\x80[\xAA-\xAE]|\xE2\x81[\xA6-\xA9]/n
    FFFD = "�".b
    MD_META = /[\\`*_{}\[\]()#+\-.!|~]/n

    module_function

    # sha256 of the canonical JSON of the whole result: the run's identity in exports.
    def run_digest(res) = Canonical.sha256_hex(Canonical.dumps(res))

    # The last iteration of each phase, in phase order.
    def last_rounds(res) = res['rounds'].to_h { |r| [r['step_id'], r] }.values

    def to_stories(res, project_key)
      stories = last_rounds(res).each_with_index.map do |r, n|
        j = r['juror']
        sid = format('STORY-%s-%04d', project_key, n + 1)
        steps = j['next_steps'].reject { |s| Gates.generic?(s) }
        if j['synthesis_status'] == 'full' && !steps.empty?
          { 'story_id' => sid, 'status' => 'ready', 'epic' => r['step_title'], 'title' => steps[0],
            'decision' => j['decision'], 'sub_tasks' => steps,
            'acceptance_criteria' => ["Implements the decision: #{j['decision']}", *j['resolved'].map { |x| "Holds: #{x}" },
                                      'A test exercises it end to end.'],
            'open_issues' => j['open_issues'].dup, 'phase' => r['step_id'], 'iteration' => r['iteration'] }
        else
          { 'story_id' => sid, 'status' => 'blocked', 'epic' => r['step_title'],
            'title' => "Decide #{r['step_title']}: the council did not reach a decision (#{j['synthesis_status']})",
            'decision' => '', 'sub_tasks' => [], 'acceptance_criteria' => [],
            'open_issues' => j['open_issues'] + (j['error'].empty? ? [] : [j['error']]),
            'phase' => r['step_id'], 'iteration' => r['iteration'] }
        end
      end
      ready = stories.count { |s| s['status'] == 'ready' }
      { 'schema' => 'smartsavant.stories/1', 'project_key' => project_key, 'run' => run_digest(res),
        'counts' => { 'stories' => stories.length, 'ready' => ready, 'blocked' => stories.length - ready },
        'stories' => stories }
    end

    # Model text made inert in Markdown: squashed to one line, control and bidirectional-control characters
    # as U+FFFD, & < > as entities, and each of \ ` * _ { } [ ] ( ) # + - . ! | ~ backslash-escaped.
    def md(text)
      s = Gates.squash(Py.str(text)).b.gsub(BAD, FFFD)
      s = s.gsub('&', '&amp;').gsub('<', '&lt;').gsub('>', '&gt;')
      Py.utf8(s.gsub(MD_META) { |c| "\\#{c}" })
    end

    def to_spec_md(res)
      h = Council.health(res)
      lines = ['# Council spec', '', "*#{md(res['idea'])}*", '',
               "Run #{run_digest(res)[0, 16]} with adapter #{md(res['adapter'])}" \
               "#{res['synthetic'] ? ' (synthetic: not a real deliberation)' : ''}. " \
               "#{h['phases_decided']} of #{h['phases']} phases decided; views #{Py.repr(h['views'])}; " \
               "merges #{Py.repr(h['merges'])}; #{h['flags']} flag(s).", '']
      unless h['decided']
        lines.push('> **Not build-ready:** at least one phase has no full synthesis. Do not build from the ' \
                   'undecided phases; re-run them with a working adapter.', '')
      end
      unless res['locked_decisions'].empty?
        lines.push('## Decisions', '', '| # | Decision |', '|---|---|')
        res['locked_decisions'].each_with_index { |d, i| lines << "| #{i + 1} | #{md(d)} |" }
        lines << ''
      end
      last_rounds(res).each do |r|
        j = r['juror']
        lines.push("## #{md(r['step_title'])}", '', "**Question:** #{md(r['question'])}", '')
        if j['synthesis_status'] == 'full'
          lines.push("**Decision:** #{md(j['decision'])}", '', md(j['merge_summary']), '')
          lines.push('**Next steps:**', '', *j['next_steps'].map { |s| "- #{md(s)}" }, '') unless j['next_steps'].empty?
        else
          lines.push("**NOT DECIDED** (#{j['synthesis_status']})#{j['error'].empty? ? '' : ": #{md(j['error'])}"}", '')
        end
        lines.push('**Open issues:**', '', *j['open_issues'].map { |s| "- #{md(s)}" }, '') unless j['open_issues'].empty?
        failed = r['views'].reject { |v| v['status'] == 'ok' }.map { |v| v['stakeholder'] }
        lines.push("*No valid view from: #{md(failed.join(', '))}.*", '') unless failed.empty?
      end
      unless res['flags'].empty?
        lines.push('## Flags', '', "#{res['flags'].length} finding(s) to review before use:", '')
        res['flags'].first(200).each do |f|
          what = Py.get(f, 'figure')
          what = Py.get(f, 'source') unless Py.truthy(what)
          what = '' unless Py.truthy(what)
          lines << "- #{Py.str(f['flag'])}: #{md(what)} (#{md(Py.get(f, 'stakeholder', ''))}, #{md(Py.get(f, 'step_id', ''))})"
        end
        lines << ''
      end
      lines.join("\n")
    end

    # Rows {instruction, input, output, meta}: methodology per default phase used; expert rows (ok views
    # without a flag) and facilitation rows (full merges) unless the run is synthetic.
    def to_trainset(res, allow_synthetic: false)
      digest = run_digest(res)
      rows = []
      flagged = res['flags'].to_h { |f| [[Py.get(f, 'step_id'), Py.get(f, 'iteration'), Py.get(f, 'stakeholder')], true] }
      learn = allow_synthetic || !res['synthetic']
      seen = {}
      res['rounds'].each do |r|
        sid = r['step_id']
        meta = { 'phase' => sid, 'iteration' => r['iteration'], 'run' => digest, 'adapter' => res['adapter'] }
        if !seen.key?(sid) && METHODOLOGY.key?(sid) && Py.lower(r['step_title']).start_with?(sid)
          seen[sid] = true
          rows << { 'instruction' => "We are at the #{r['step_title']} phase of a design council. How should I facilitate it?",
                    'input' => '', 'output' => METHODOLOGY[sid], 'meta' => meta.merge('kind' => 'methodology') }
        end
        next unless learn
        r['views'].each do |v|
          next unless v['status'] == 'ok' && !flagged.key?([sid, r['iteration'], v['stakeholder']])
          rows << { 'instruction' => "As the #{v['stakeholder']} on a design council, give your view on: #{r['question']}",
                    'input' => res['idea'], 'output' => Json.py(v.slice(*VIEW_FIELDS), sort_keys: true),
                    'meta' => meta.merge('kind' => 'expert', 'stakeholder' => v['stakeholder']) }
        end
        j = r['juror']
        next unless j['synthesis_status'] == 'full'
        rows << { 'instruction' => "Merge the panel for the #{r['step_title']} phase as the juror.",
                  'input' => Json.py(r['views'].select { |v| v['status'] == 'ok' }.map { |v| v['contribution'] }),
                  'output' => Json.py({ 'decision' => j['decision'], 'merge_summary' => j['merge_summary'],
                                        'resolved' => j['resolved'], 'next_steps' => j['next_steps'] }, sort_keys: true),
                  'meta' => meta.merge('kind' => 'facilitation', 'synthesis' => 'full') }
      end
      rows
    end

    # A Mermaid label: squashed, control characters as U+FFFD, at most 80 code points, & " < > replaced, quoted.
    def label(t)
      s = Py.head(Py.utf8(Gates.squash(Py.str(t)).b.gsub(BAD, FFFD)), 80).b
      Py.utf8("\"#{s.gsub('&', '&amp;').gsub('"', "'").gsub('<', '&lt;').gsub('>', '&gt;')}\"".b)
    end

    def to_mermaid(res)
      lines = ['flowchart TD', '  start([Idea])']
      prev = 'start'
      res['decision_tree'].each_with_index do |n, i|
        nid = "n#{i}"
        r = res['rounds'][i]
        st = r['juror']['synthesis_status']
        lab = st == 'full' ? n['decision'] : "NOT DECIDED: #{st}"
        lines << "  #{nid}{#{label("#{r['step_title']} ##{Py.str(n['iteration'])}")}}"
        lines << "  #{prev} --> #{nid}"
        lines << "  #{nid}d[#{label(lab)}]"
        lines << "  #{nid} --> #{nid}d"
        prev = nid
      end
      "#{lines.join("\n")}\n"
    end
  end
end
