# frozen_string_literal: true

# The data representation of this port and the Python behaviour the reference relies on.
#
# JSON data is nil, true, false, Integer, Float, String, Array and Hash (String keys, insertion order).
# Strings are UTF-8. An unpaired surrogate (JSON "\ud800") is kept as its 3-byte generalised UTF-8 form
# (ED A0 80, "WTF-8"); such a string is not valid_encoding?, so every helper here works on its bytes.
# Lengths and slices count code points.
module SmartSavant
  module Py
    NONCONT = "\x00-\x7F\xC0-\xFF".b
    LONE = /\xED[\xA0-\xBF][\x80-\xBF]/n
    CHAR = /[\xC0-\xFF][\x80-\xBF]*|[\x00-\xBF]/n

    module_function

    # The UTF-8 bytes of s as a binary string (for the /n patterns).
    def bin(s) = s.b

    # A binary string read back as UTF-8.
    def utf8(b) = b.dup.force_encoding(Encoding::UTF_8)

    # len(s) in code points.
    def len(s) = s.valid_encoding? ? s.length : s.b.count(NONCONT)

    # s[:n] in code points.
    def head(s, n)
      return s[0, n] if s.valid_encoding?
      return s if s.bytesize <= n
      utf8(s.b.scan(CHAR).first(n).join)
    end

    # The code points of s, each as a String.
    def chars(s) = s.valid_encoding? ? s.chars : s.b.scan(CHAR).map { |c| utf8(c) }

    def ord(c)
      return c.ord if c.valid_encoding?
      b = c.bytes
      n = b[0] >= 0xF0 ? 3 : b[0] >= 0xE0 ? 2 : b[0] >= 0xC0 ? 1 : 0
      cp = n.zero? ? b[0] : b[0] & (0x3F >> n)
      b[1..n].each { |x| cp = (cp << 6) | (x & 0x3F) }
      cp
    end

    # chr(cp) as UTF-8 (a surrogate as its 3-byte form).
    def chr(cp)
      return cp.chr(Encoding::UTF_8) unless cp.between?(0xD800, 0xDFFF)
      utf8([0xE0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3F), 0x80 | (cp & 0x3F)].pack('C*'))
    end

    def lone?(s) = s.b.match?(/\xED[\xA0-\xBF]/n)

    def replace_lone(s, by) = lone?(s) ? utf8(s.b.gsub(LONE, by.b)) : s

    # UTF-8 text (surrogate code points allowed, as a Python str can hold them); other encodings converted.
    # Returns nil when s is not text.
    def text(s)
      if s.encoding == Encoding::UTF_8 || s.encoding == Encoding::BINARY || s.encoding == Encoding::US_ASCII
        u = utf8(s)
        return u if u.valid_encoding? || utf8(u.b.gsub(LONE, 'a')).valid_encoding?
        return nil
      end
      s.encode(Encoding::UTF_8)
    rescue EncodingError
      nil
    end

    # s.encode('utf-8', 'replace'): lone surrogates as '?'; other invalid bytes too.
    def scrub(s)
      t = text(s) || utf8(s.b)
      replace_lone(t, '?').scrub('?')
    end

    # str.lower() (full Unicode lower-casing).
    def lower(s) = replace_lone(s, '?').scrub('?').downcase

    # Python orders str by code point: the byte order of UTF-8.
    def sorted(a) = a.sort { |x, y| x.b <=> y.b }

    # bool(v).
    def truthy(v)
      case v
      when nil, false then false
      when Integer then v != 0
      when Float then v != 0.0 || v.nan?
      when String, Array, Hash then !v.empty?
      else true
      end
    end

    # Iterating a value as Python does: a str gives its characters, a dict its keys.
    def iter(v)
      case v
      when String then chars(v)
      when Array then v
      when Hash then v.keys
      else []
      end
    end

    # dict.get(k, d) on any value (nil when it is not a Hash).
    def get(o, k, d = nil) = o.is_a?(Hash) && o.key?(k) ? o[k] : d

    # repr(float).
    def float_repr(f)
      return 'nan' if f.nan?
      return f.positive? ? 'inf' : '-inf' if f.infinite?
      return (1.0 / f).negative? ? '-0.0' : '0.0' if f.zero?
      e = nil
      (0..16).each do |p|
        e = format("%.#{p}e", f)
        break if e.to_f == f
      end
      m = /\A(-?)(\d)(?:\.(\d+))?e([+-]\d+)\z/.match(e)
      sign = m[1]
      digits = (m[2] + (m[3] || '')).sub(/0+\z/, '')
      digits = '0' if digits.empty?
      exp = m[4].to_i
      if exp >= -4 && exp < 16
        point = exp + 1
        return "#{sign}0.#{'0' * -point}#{digits}" if point <= 0
        return "#{sign}#{digits}#{'0' * (point - digits.length)}.0" if point >= digits.length
        return "#{sign}#{digits[0, point]}.#{digits[point..]}"
      end
      mant = digits[0] + (digits.length > 1 ? ".#{digits[1..]}" : '')
      "#{sign}#{mant}e#{exp.negative? ? '-' : '+'}#{format('%02d', exp.abs)}"
    end

    UNPRINTABLE = /\A[\p{Cc}\p{Cf}\p{Co}\p{Cn}\p{Zl}\p{Zp}\p{Zs}]\z/

    # repr(str). Printability of non-ASCII characters follows Ruby's Unicode tables.
    def repr_str(s)
      q = s.include?("'") && !s.include?('"') ? '"' : "'"
      out = +q
      chars(s).each do |ch|
        c = ord(ch)
        out << if ch == q || ch == '\\' then "\\#{ch}"
               elsif ch == "\n" then '\\n'
               elsif ch == "\r" then '\\r'
               elsif ch == "\t" then '\\t'
               elsif c < 0x20 || (c >= 0x7F && c <= 0xA0) then format('\\x%02x', c)
               elsif c.between?(0xD800, 0xDFFF) then format('\\u%04x', c)
               elsif c > 0x7F && UNPRINTABLE.match?(ch)
                 c < 0x100 ? format('\\x%02x', c) : c < 0x10000 ? format('\\u%04x', c) : format('\\U%08x', c)
               else ch
               end
      end
      out << q
    end

    def repr(v)
      case v
      when nil then 'None'
      when true then 'True'
      when false then 'False'
      when Integer then v.to_s
      when Float then float_repr(v)
      when String then repr_str(v)
      when Array then "[#{v.map { |x| repr(x) }.join(', ')}]"
      when Hash then "{#{v.map { |k, x| "#{repr(k)}: #{repr(x)}" }.join(', ')}}"
      when Symbol then repr_str(v.to_s)
      else v.class.name.to_s
      end
    end

    # str(v).
    def str(v) = v.is_a?(String) ? v : repr(v)

    # A deep copy of JSON data (copy.deepcopy for what adapters see).
    def copy(v)
      case v
      when Hash then v.each_with_object({}) { |(k, x), o| o[k] = copy(x) }
      when Array then v.map { |x| copy(x) }
      when String then v.dup
      else v
      end
    end

    # JSON value equality as the vectors are compared (key order ignored); true is not 1.
    def same(a, b)
      return a.equal?(b) if [true, false, nil].include?(a) || [true, false, nil].include?(b)
      return a == b if a.is_a?(Numeric) && b.is_a?(Numeric)
      return a.is_a?(String) && b.is_a?(String) && a.b == b.b if a.is_a?(String) || b.is_a?(String)
      if a.is_a?(Array) && b.is_a?(Array)
        return a.length == b.length && a.each_index.all? { |i| same(a[i], b[i]) }
      end
      if a.is_a?(Hash) && b.is_a?(Hash)
        return a.length == b.length && a.all? { |k, x| b.key?(k) && same(x, b[k]) }
      end
      false
    end
  end

  # JSON text in and out: a strict reader (as Python's json.loads) and Python's json.dumps formatting.
  module Json
    class Error < StandardError; end

    MAX_DEPTH = 1000

    # Parse JSON text into Hash / Array / scalars. Integers stay Integer, numbers with a fraction or an
    # exponent are Float, NaN / Infinity are accepted as by Python, an unpaired \ud800-style escape is kept
    # (see the header of this file). Raises Json::Error.
    def self.decode(text)
      b = text.b
      raise Error, 'not UTF-8 text' unless Py.utf8(b).valid_encoding?
      Reader.new(b).parse
    end

    ESC = { '"' => '\\"', '\\' => '\\\\', "\n" => '\\n', "\r" => '\\r', "\t" => '\\t', "\b" => '\\b', "\f" => '\\f' }.freeze

    def self.str(s, ensure_ascii)
      out = s.b.gsub(/[\x00-\x1F"\\]/n) { |c| ESC[c] || format('\\u%04x', c.ord) }
      out = Py.utf8(out)
      if ensure_ascii && !out.b.ascii_only?
        out = Py.chars(out).map do |ch|
          c = Py.ord(ch)
          if c < 0x80 then ch
          elsif c < 0x10000 then format('\\u%04x', c)
          else
            c -= 0x10000
            format('\\u%04x\\u%04x', 0xD800 | (c >> 10), 0xDC00 | (c & 0x3FF))
          end
        end.join
      end
      "\"#{out}\""
    end

    # json.dumps(v, ensure_ascii=ensure_ascii, sort_keys=sort_keys) with the ", " and ": " separators.
    def self.py(v, sort_keys: false, ensure_ascii: false)
      case v
      when nil then 'null'
      when true then 'true'
      when false then 'false'
      when Integer then v.to_s
      when Float
        return 'NaN' if v.nan?
        return v.positive? ? 'Infinity' : '-Infinity' if v.infinite?
        Py.float_repr(v)
      when String then str(v, ensure_ascii)
      when Array then "[#{v.map { |x| py(x, sort_keys: sort_keys, ensure_ascii: ensure_ascii) }.join(', ')}]"
      when Hash
        keys = sort_keys ? Py.sorted(v.keys.map(&:to_s)) : v.keys.map(&:to_s)
        h = v.transform_keys(&:to_s)
        "{#{keys.map { |k| "#{str(k, ensure_ascii)}: #{py(h[k], sort_keys: sort_keys, ensure_ascii: ensure_ascii)}" }.join(', ')}}"
      else raise TypeError, "Object of type #{v.class} is not JSON serializable"
      end
    end

    # The reader behind Json.decode (works on the bytes).
    class Reader
      NUM = /-?(?:0|[1-9][0-9]*)(\.[0-9]+)?([eE][-+]?[0-9]+)?/n

      def initialize(b)
        @s = b
        @i = 0
        @n = b.bytesize
        @depth = 0
      end

      def parse
        ws
        v = value
        ws
        fail!('extra data') unless @i == @n
        v
      end

      private

      def fail!(m) = raise(Error, "#{m} at byte #{@i}")

      def ws
        @i += 1 while @i < @n && (c = @s.getbyte(@i)) && (c == 32 || c == 9 || c == 10 || c == 13)
      end

      def lit(w)
        return false unless @s.byteslice(@i, w.bytesize) == w
        @i += w.bytesize
        true
      end

      def value
        fail!('expecting value') if @i >= @n
        c = @s.getbyte(@i)
        if c == 123 || c == 91
          @depth += 1
          fail!('nested too deeply') if @depth > MAX_DEPTH
          v = c == 123 ? object : array
          @depth -= 1
          return v
        end
        return string if c == 34
        return true if lit('true')
        return false if lit('false')
        return nil if lit('null')
        return Float::NAN if lit('NaN')
        return Float::INFINITY if lit('Infinity')
        return -Float::INFINITY if lit('-Infinity')
        m = NUM.match(@s, @i)
        fail!('expecting value') unless m && m.begin(0) == @i
        @i = m.end(0)
        m[1] || m[2] ? Float(m[0]) : Integer(m[0], 10)
      end

      def peek = @s.getbyte(@i)

      def object
        h = {}
        @i += 1
        ws
        if peek == 125
          @i += 1
          return h
        end
        while true
          ws
          fail!('expecting property name') unless peek == 34
          k = string
          ws
          fail!("expecting ':'") unless peek == 58
          @i += 1
          ws
          h[k] = value
          ws
          c = peek
          @i += 1
          return h if c == 125
          fail!("expecting ',' or '}'") unless c == 44
        end
      end

      def array
        a = []
        @i += 1
        ws
        if peek == 93
          @i += 1
          return a
        end
        while true
          ws
          a << value
          ws
          c = peek
          @i += 1
          return a if c == 93
          fail!("expecting ',' or ']'") unless c == 44
        end
      end

      def hex4(at)
        h = @s.byteslice(at, 4)
        h && h.bytesize == 4 && h.match?(/\A\h{4}\z/n) ? h.to_i(16) : nil
      end

      def string
        @i += 1
        out = +''.b
        while true
          j = @i
          j += 1 while j < @n && (c = @s.getbyte(j)) != 34 && c != 92 && c >= 0x20
          out << @s.byteslice(@i, j - @i)
          @i = j
          fail!('unterminated string') if @i >= @n
          c = @s.getbyte(@i)
          if c == 34
            @i += 1
            return Py.utf8(out)
          end
          fail!('invalid control character') unless c == 92
          e = @s.getbyte(@i + 1)
          @i += 2
          case e
          when 34 then out << '"'
          when 92 then out << '\\'
          when 47 then out << '/'
          when 98 then out << "\b"
          when 102 then out << "\f"
          when 110 then out << "\n"
          when 114 then out << "\r"
          when 116 then out << "\t"
          when 117
            cp = hex4(@i)
            fail!('invalid \\uXXXX escape') if cp.nil?
            @i += 4
            if cp.between?(0xD800, 0xDBFF) && @s.byteslice(@i, 2) == '\\u'
              lo = hex4(@i + 2)
              if lo&.between?(0xDC00, 0xDFFF)
                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00)
                @i += 6
              end
            end
            out << Py.chr(cp).b
          else fail!('invalid escape')
          end
        end
      end
    end
  end
end
