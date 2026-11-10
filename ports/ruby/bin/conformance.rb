# frozen_string_literal: true

# ruby bin/conformance.rb [DIR]: runs spec/conformance/*.json against the Ruby port, check for check as
# src/sf_smartsavant/conformance.py does. Prints the failures and a last line "ruby: N/N vectors passed".
require_relative '../lib/smartsavant'

module SmartSavant
  module Conformance
    module_function

    def main(dir)
      load = ->(n) { Json.decode(File.binread(File.join(dir, n))) }
      show = ->(v) { Py.head(Json.py(v, sort_keys: true), 300) }
      total = 0
      fails = []
      check = lambda do |name, want, &got|
        total += 1
        begin
          g = got.call
        rescue StandardError => e
          fails << "#{name}: raised #{e.class}: #{e.message}"
          next
        end
        fails << "#{name}: got #{show.call(g)}, want #{show.call(want)}" unless Py.same(g, want)
      end

      load.call('canonical.json')['cases'].each do |c|
        check.call("canonical/#{c['name']}", [c['canonical'], c['sha256']]) do
          t = Canonical.dumps(c['input'])
          [t, Canonical.sha256_hex(t)]
        end
      end
      load.call('figures.json')['cases'].each_with_index { |c, i| check.call("figures/#{i}", c['figures']) { Gates.figures(c['text']) } }
      g = load.call('gates.json')['cases']
      g['flag_unsourced'].each do |c|
        check.call("flag_unsourced/#{c['name']}", c['flags']) { Gates.flag_unsourced(c['view'], c['allowed']) }
      end
      { 'is_generic' => :generic?, 'is_fallback' => :fallback?, 'has_placeholder_code' => :placeholder_code?,
        'compliance_block' => :compliance_block, 'squash' => :squash }.each do |fn, m|
        g[fn].each_with_index { |c, i| check.call("#{fn}/#{i}", c['result']) { Gates.public_send(m, c['text']) } }
      end
      g['synthesis_ok'].each { |c| check.call("synthesis_ok/#{c['name']}", c['result']) { Gates.synthesis_ok(c['merge']) } }
      s = load.call('schema.json')['cases']
      s['view'].each { |c| check.call("schema/view/#{c['name']}", c['errors']) { Schema.check_view(c['input']) } }
      s['merge'].each { |c| check.call("schema/merge/#{c['name']}", c['errors']) { Schema.check_merge(c['input']) } }
      load.call('runs.json')['cases'].each do |c|
        ph = Py.truthy(c['phases']) ? c['phases'].map { |p| [p['step_id'], p['title'], p['question']] } : nil
        begin
          res = Council.new(c['idea'], ScriptedAdapter.new(c['script']), roster: c['roster'], phases: ph, sources: c['sources'],
                                                                         max_loops: c['max_loops'], retries: c['retries']).run
        rescue StandardError => e
          c['expect'].each_key do |field|
            total += 1
            fails << "runs/#{c['name']}/#{field}: run raised #{e.class}: #{e.message}"
          end
          next
        end
        k = c['project_key']
        got = {
          'council' => -> { res }, 'digest' => -> { Exporters.run_digest(res) }, 'health' => -> { Council.health(res) },
          'stories' => -> { Exporters.to_stories(res, k) }, 'spec_md' => -> { Exporters.to_spec_md(res) },
          'mermaid' => -> { Exporters.to_mermaid(res) }, 'trainset' => -> { Exporters.to_trainset(res) },
          'trainset_all' => -> { Exporters.to_trainset(res, allow_synthetic: true) }, 'audit' => -> { Audit.audit_result(res) }
        }
        c['expect'].each do |field, want|
          check.call("runs/#{c['name']}/#{field}", want) { got.fetch(field).call }
        end
      end
      a = load.call('audit.json')['cases']
      a['import'].each { |c| check.call("import/#{c['name']}", c['council']) { Importer.from_smartsignal(c['input']) } }
      a['council'].each do |c|
        check.call("audit/council/#{c['name']}", c['report']) { Audit.audit_result(Importer.from_smartsignal(c['input'])) }
      end
      a['stories'].each { |c| check.call("audit/stories/#{c['name']}", c['report']) { Audit.audit_stories(c['input']) } }
      a['trainset'].each { |c| check.call("audit/trainset/#{c['name']}", c['report']) { Audit.audit_trainset(c['input']) } }

      fails.each { |f| $stdout.write(f.b, "\n") }
      passed = total - fails.length
      puts fails.empty? ? "ruby: #{total}/#{total} vectors passed" : "ruby: #{passed}/#{total} vectors passed (#{fails.length} failed)"
      fails.empty? ? 0 : 1
    end
  end
end

begin
  exit SmartSavant::Conformance.main(ARGV[0] || File.expand_path('../../../spec/conformance', __dir__))
rescue StandardError, ScriptError => e
  warn e.full_message
  puts 'ruby: FAILED'
  exit 1
end
