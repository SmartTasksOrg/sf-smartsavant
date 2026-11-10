# frozen_string_literal: true

# Validation of what an adapter returns, and of council files. The error strings and their order are part of
# the contract (spec/conformance/schema.json).
module SmartSavant
  class NotPlainData < ArgumentError; end

  module Schema
    MAX_STR = 8000
    MAX_DELTA = 20_000
    MAX_ITEMS = 20
    MAX_ITEM = 2000
    MAX_DEPTH = 32
    MAX_TOTAL = 1_000_000
    SOURCE_ID = %r{\A[A-Za-z0-9._:/#@-]{1,200}\z}n
    W5H1_KEYS = %w[who what where when why how].freeze
    VIEW_KEYS = %w[w5h1 done_so_far missing devils_advocate contribution sources].freeze
    MERGE_KEYS = %w[merge_summary decision resolved open_issues next_steps reversals spec_delta loop_recommended].freeze
    MERGE_REQUIRED = (MERGE_KEYS - ['reversals']).freeze
    BLANK = /\A[\x09-\x0D\x20]*\z/n

    module_function

    # A copy of an adapter answer made only of nil, true, false, Integer, Float, String, Array and Hash, built
    # before anything else so that what is validated is exactly what is used (schema.plain in Python).
    # Exact classes only (no subclasses). Symbol keys are taken as their names. Refused with NotPlainData:
    # other types and key types, two keys with the same name, strings that are not text, nesting deeper
    # than 32 and more than 1,000,000 code points of text (keys included). Strings are returned as UTF-8.
    def plain(obj)
      budget = MAX_TOTAL
      text = lambda do |s|
        t = Py.text(s)
        raise NotPlainData, 'strings must be UTF-8 text' if t.nil?
        budget -= Py.len(t)
        t
      end
      go = lambda do |x, depth|
        raise NotPlainData, 'nested deeper than 32 levels' if depth > MAX_DEPTH
        return x if x.nil? || true.equal?(x) || false.equal?(x) || x.instance_of?(Integer) || x.instance_of?(Float)
        if x.instance_of?(String)
          t = text.call(x)
          raise NotPlainData, 'more than 1,000,000 characters of text' if budget.negative?
          return t
        end
        return x.map { |v| go.call(v, depth + 1) } if x.instance_of?(Array)
        raise NotPlainData, "#{x.class} is not JSON data" unless x.instance_of?(Hash)
        out = {}
        x.each do |k, v|
          raise NotPlainData, 'object keys must be strings' unless k.instance_of?(String) || k.instance_of?(Symbol)
          k = text.call(k.to_s)
          raise NotPlainData, "duplicate key #{k}" if out.key?(k)
          out[k] = go.call(v, depth + 1)
        end
        raise NotPlainData, 'more than 1,000,000 characters of text' if budget.negative?
        out
      end
      go.call(obj, 0)
    end

    # An unknown key as shown in an error: control characters dropped, unpaired surrogates as U+FFFD, then at
    # most 40 code points.
    def show_key(k)
      k = Py.utf8(Py.str(k).b.gsub(/[\x00-\x1F\x7F]|\xC2[\x80-\x9F]/n, ''))
      k = Py.replace_lone(k, "\uFFFD")               # error text is recorded: it must stay encodable
      Py.head(k, 40) + (Py.len(k) > 40 ? '...' : '')
    end

    def blank?(s) = BLANK.match?(s.b)

    def check_str(errs, where, v, limit = MAX_STR, nonempty: false)
      if !v.is_a?(String) then errs << "#{where} must be a string"
      elsif Py.len(v) > limit then errs << "#{where} is longer than #{limit} characters"
      elsif v.b.include?("\0") then errs << "#{where} contains a NUL character"
      elsif Py.lone?(v) then errs << "#{where} contains an unpaired surrogate"
      elsif nonempty && blank?(v) then errs << "#{where} must not be empty"
      end
    end

    def check_list(errs, where, v, pattern = nil)
      unless v.is_a?(Array)
        errs << "#{where} must be a list of strings"
        return
      end
      errs << "#{where} has more than #{MAX_ITEMS} items" if v.length > MAX_ITEMS
      v.first(MAX_ITEMS + 1).each_with_index do |x, i|
        if !x.is_a?(String) || blank?(x) then errs << "#{where}[#{i}] must be a non-empty string"
        elsif Py.len(x) > MAX_ITEM then errs << "#{where}[#{i}] is longer than #{MAX_ITEM} characters"
        elsif pattern && !pattern.match?(x.b) then errs << "#{where}[#{i}] is not a valid source id"
        elsif x.b.include?("\0") then errs << "#{where}[#{i}] contains a NUL character"
        elsif Py.lone?(x) then errs << "#{where}[#{i}] contains an unpaired surrogate"
        end
      end
    end

    def same_keys?(o, keys) = o.is_a?(Hash) && o.length == keys.length && keys.all? { |k| o.key?(k) }

    def key_errors(errs, o, allowed, required)
      extra = Py.sorted(o.keys.reject { |k| allowed.include?(k) })
      errs << "unknown keys (#{extra.length}): #{extra.first(5).map { |k| show_key(k) }.join(', ')}" unless extra.empty?
      missing = Py.sorted(required.reject { |k| o.key?(k) })
      return true if missing.empty?
      errs << "missing keys: #{missing.join(', ')}"
      false
    end

    # [] when v is a valid view; otherwise the reasons, first problem first.
    def check_view(v)
      return ['a view must be a JSON object'] unless v.is_a?(Hash)
      errs = []
      return errs unless key_errors(errs, v, VIEW_KEYS, VIEW_KEYS)
      w = v['w5h1']
      if same_keys?(w, W5H1_KEYS)
        W5H1_KEYS.each { |k| check_str(errs, "w5h1.#{k}", w[k]) }
      else
        errs << 'w5h1 must be an object with exactly who, what, where, when, why, how'
      end
      check_str(errs, 'done_so_far', v['done_so_far'])
      if v['missing'].is_a?(String) then check_str(errs, 'missing', v['missing'])
      else check_list(errs, 'missing', v['missing'])
      end
      check_str(errs, 'devils_advocate', v['devils_advocate'], nonempty: true)
      check_str(errs, 'contribution', v['contribution'])
      check_list(errs, 'sources', v['sources'], SOURCE_ID)
      errs
    end

    # [] when m is a valid merge; otherwise the reasons, first problem first.
    def check_merge(m)
      return ['a merge must be a JSON object'] unless m.is_a?(Hash)
      errs = []
      return errs unless key_errors(errs, m, MERGE_KEYS, MERGE_REQUIRED)
      check_str(errs, 'merge_summary', m['merge_summary'], nonempty: true)
      check_str(errs, 'decision', m['decision'])
      %w[resolved open_issues next_steps].each { |k| check_list(errs, k, m[k]) }
      check_list(errs, 'reversals', m['reversals']) if m.key?('reversals')
      check_str(errs, 'spec_delta', m['spec_delta'], MAX_DELTA)
      errs << 'loop_recommended must be true or false' unless m['loop_recommended'] == true || m['loop_recommended'] == false
      errs
    end

    # A valid view with missing always a list (a copy).
    def normalize_view(v)
      out = Py.copy(v)
      out['missing'] = blank?(out['missing']) ? [] : [out['missing']] if out['missing'].is_a?(String)
      out
    end

    # ------------------------------------------------------------ council files
    STATUS_V = %w[ok failed offline].freeze
    STATUS_J = %w[full thin recovered failed offline no_views].freeze
    TOP = %w[schema idea adapter synthetic stakeholders sources rounds decision_tree locked_decisions flags].freeze
    VIEW_REC = { 'stakeholder' => :s, 'role' => :s, 'status' => :s, 'w5h1' => :w, 'done_so_far' => :s, 'missing' => :l,
                 'devils_advocate' => :s, 'contribution' => :s, 'sources' => :l, 'error' => :s }.freeze
    JUROR_REC = { 'synthesis_status' => :s, 'merge_summary' => :s, 'decision' => :s, 'resolved' => :l, 'open_issues' => :l,
                  'next_steps' => :l, 'reversals' => :l, 'spec_delta' => :s, 'loop_recommended' => :b, 'error' => :s }.freeze
    ROUND_REC = { 'step_id' => :s, 'step_title' => :s, 'iteration' => :i, 'question' => :s, 'views' => :L,
                  'juror' => :J }.freeze
    NODE_REC = { 'step_id' => :s, 'iteration' => :i, 'branches' => :l, 'decision' => :s, 'next_steps' => :l }.freeze

    def good_str?(x) = x.is_a?(String) && !Py.lone?(x)

    def str_list?(v) = v.is_a?(Array) && v.all? { |x| good_str?(x) }

    def type_ok?(t, v)
      case t
      when :s then good_str?(v)
      when :i then v.is_a?(Integer) && v >= 1
      when :b then v == true || v == false
      when :l then str_list?(v)
      when :w then same_keys?(v, W5H1_KEYS) && str_list?(v.values)
      when :L then v.is_a?(Array)
      when :J then v.is_a?(Hash)
      end
    end

    def typed(errs, where, d, spec)
      unless same_keys?(d, spec.keys)
        errs << "#{where} must have exactly the keys #{spec.keys.join(', ')}"
        return
      end
      spec.each { |k, t| errs << "#{where}.#{k} has the wrong type" unless type_ok?(t, d[k]) }
    end

    # A council.json written by an engine of this package: exact keys and JSON types everywhere, known
    # statuses, no unpaired surrogates. At most 50 errors.
    def check_council(d)
      return ['a council file must be a JSON object'] unless d.is_a?(Hash)
      return ['schema must be "smartsavant.council/1"'] unless d['schema'] == 'smartsavant.council/1'
      return ["top-level keys must be exactly #{Py.sorted(TOP).join(', ')}"] unless same_keys?(d, TOP)
      errs = []
      %w[idea adapter].each { |k| errs << "#{k} must be a string" unless good_str?(d[k]) }
      errs << 'synthetic must be true or false' unless d['synthetic'] == true || d['synthetic'] == false
      %w[stakeholders sources locked_decisions].each { |k| errs << "#{k} must be a list of strings" unless str_list?(d[k]) }
      unless d['rounds'].is_a?(Array) && d['decision_tree'].is_a?(Array) && d['flags'].is_a?(Array)
        return errs + ['rounds, decision_tree and flags must be lists']
      end
      errs << 'decision_tree must have one node per round' unless d['decision_tree'].length == d['rounds'].length
      d['rounds'].each_with_index do |r, i|
        w = "rounds[#{i}]"
        typed(errs, w, r, ROUND_REC)
        next unless r.is_a?(Hash) && r['views'].is_a?(Array) && r['juror'].is_a?(Hash)
        r['views'].each_with_index do |v, j|
          typed(errs, "#{w}.views[#{j}]", v, VIEW_REC)
          if v.is_a?(Hash) && !STATUS_V.include?(v['status'])
            errs << "#{w}.views[#{j}].status must be one of #{STATUS_V.join(', ')}"
          end
        end
        typed(errs, "#{w}.juror", r['juror'], JUROR_REC)
        unless STATUS_J.include?(r['juror']['synthesis_status'])
          errs << "#{w}.juror.synthesis_status must be one of #{STATUS_J.join(', ')}"
        end
      end
      d['decision_tree'].each_with_index { |n, i| typed(errs, "decision_tree[#{i}]", n, NODE_REC) }
      d['flags'].each_with_index do |f, i|
        ok = f.is_a?(Hash) && f.values.all? { |x| good_str?(x) || x.is_a?(Integer) }
        errs << "flags[#{i}] must be an object of strings and integers" unless ok
      end
      errs.first(50)
    end
  end
end
