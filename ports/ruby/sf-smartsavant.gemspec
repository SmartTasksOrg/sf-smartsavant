# SmartSavant for Ruby: the council engine, gates, schema checks, adapters, exporters, importer and audit.
# Not published on RubyGems; use it from a clone (ruby -Iports/ruby/lib).
Gem::Specification.new do |s|
  s.name = 'sf-smartsavant'
  s.version = '0.1.0'
  s.summary = 'SmartSavant for Ruby: the model-agnostic design-council engine, its gates, exporters, importer and audit.'
  s.license = 'Apache-2.0'
  s.authors = ['SmartTasks Lab']
  s.files = Dir['lib/**/*']
  s.required_ruby_version = '>= 3.2'
  s.metadata['allowed_push_host'] = 'https://example.invalid' # not published: refuse `gem push`
end
