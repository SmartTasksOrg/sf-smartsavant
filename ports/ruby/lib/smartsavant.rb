# frozen_string_literal: true

# SmartSavant for Ruby: the engine, gates, schema checks, adapters (stub, scripted, replay), exporters,
# importer and audit of the Python reference (src/sf_smartsavant), in module SmartSavant.
#   require 'smartsavant'   # with -Iports/ruby/lib
module SmartSavant
  VERSION = '0.1.1'
end

require_relative 'smartsavant/py'
require_relative 'smartsavant/gates'
require_relative 'smartsavant/schema'
require_relative 'smartsavant/engine'
require_relative 'smartsavant/exporters'
require_relative 'smartsavant/audit'
