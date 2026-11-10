//! JSON values as the Python reference sees them: objects keep their key order (Python dicts do),
//! integers and floats stay distinct (1 is not 1.0, true is not 1), and unpaired UTF-16 surrogates
//! survive parsing.
//!
//! Rust strings hold UTF-8, which cannot encode U+D800-U+DFFF. The parser therefore maps an unpaired
//! surrogate escape (`"\ud800"`) to the code point U+10F800 + (s - 0xD800), in the last 2048 code
//! points of plane 16 (private use). Every check for unpaired surrogates looks for that range, and the
//! writers turn it back into a `\udXXX` escape. A real character from U+10F800-U+10FFFF in the input is
//! therefore treated as an unpaired surrogate (see README.md).
use std::collections::HashMap;
use std::fmt::Write as _;

/// A JSON value.
#[derive(Clone, Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    /// An integer outside the i64 range, as its decimal text.
    BigInt(String),
    Float(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Object),
}

/// The null value, for references to a missing key.
pub static NULL: Value = Value::Null;

/// A JSON object that keeps the insertion order of its keys (a repeated key keeps its first position).
#[derive(Clone, Debug, Default)]
pub struct Object {
    keys: Vec<String>,
    vals: HashMap<String, Value>,
}

impl Object {
    pub fn new() -> Object {
        Object::default()
    }
    /// Sets `k` to `v`; a new key goes last, an existing key keeps its position.
    pub fn set(&mut self, k: impl Into<String>, v: impl Into<Value>) -> &mut Object {
        let k = k.into();
        if !self.vals.contains_key(&k) {
            self.keys.push(k.clone());
        }
        self.vals.insert(k, v.into());
        self
    }
    /// Builder form of `set`.
    pub fn with(mut self, k: impl Into<String>, v: impl Into<Value>) -> Object {
        self.set(k, v);
        self
    }
    pub fn get(&self, k: &str) -> Option<&Value> {
        self.vals.get(k)
    }
    /// `dict.get(k, d)`: a present key with a null value gives null, not `d`.
    pub fn get_or<'a>(&'a self, k: &str, d: &'a Value) -> &'a Value {
        self.vals.get(k).unwrap_or(d)
    }
    /// `dict.get(k)`: null when missing.
    pub fn at(&self, k: &str) -> &Value {
        self.vals.get(k).unwrap_or(&NULL)
    }
    pub fn has(&self, k: &str) -> bool {
        self.vals.contains_key(k)
    }
    pub fn keys(&self) -> &[String] {
        &self.keys
    }
    pub fn len(&self) -> usize {
        self.keys.len()
    }
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
    /// Pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.keys.iter().map(move |k| (k, &self.vals[k]))
    }
}

/// Builds an object from pairs.
pub fn obj(pairs: Vec<(&str, Value)>) -> Value {
    let mut o = Object::new();
    for (k, v) in pairs {
        o.set(k, v);
    }
    Value::Obj(o)
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::Str(s.to_string())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::Str(s)
    }
}
impl From<&String> for Value {
    fn from(s: &String) -> Value {
        Value::Str(s.clone())
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}
impl From<i64> for Value {
    fn from(n: i64) -> Value {
        Value::Int(n)
    }
}
impl From<usize> for Value {
    fn from(n: usize) -> Value {
        Value::Int(n as i64)
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Value {
        Value::Arr(v)
    }
}
impl From<Object> for Value {
    fn from(o: Object) -> Value {
        Value::Obj(o)
    }
}

/// A list of strings as a JSON array.
pub fn str_list(xs: &[String]) -> Value {
    Value::Arr(xs.iter().map(|x| Value::Str(x.clone())).collect())
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_obj(&self) -> Option<&Object> {
        match self {
            Value::Obj(o) => Some(o),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
    /// True when this is the string `s`.
    pub fn is_str(&self, s: &str) -> bool {
        self.as_str() == Some(s)
    }
}

// ---------------------------------------------------------------- unpaired surrogates

const SURROGATE_BASE: u32 = 0x10F800;

/// True for the character that stands for an unpaired surrogate (see the module comment).
pub fn is_lone_surrogate(c: char) -> bool {
    (c as u32) >= SURROGATE_BASE
}

/// The character that stands for the surrogate code unit `u` (0xD800-0xDFFF).
pub fn lone_surrogate(u: u32) -> char {
    char::from_u32(SURROGATE_BASE + (u - 0xD800)).unwrap()
}

fn surrogate_unit(c: char) -> u32 {
    c as u32 - SURROGATE_BASE + 0xD800
}

pub fn has_lone_surrogate(s: &str) -> bool {
    s.chars().any(is_lone_surrogate)
}

pub fn replace_lone_surrogates(s: &str, by: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if is_lone_surrogate(c) {
            out.push_str(by);
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------- parser

const MAX_PARSE_DEPTH: usize = 1000;

/// Reads one JSON document as Python's `json.loads` does: objects keep their key order (a repeated key
/// keeps its first position and its last value), integers are `Int` (`BigInt` beyond i64), numbers with
/// a fraction or exponent are `Float`, NaN, Infinity and -Infinity are accepted, control characters in
/// strings are refused, and unpaired surrogate escapes are kept (see the module comment).
pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser { s: text.as_bytes(), i: 0 };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.fail("extra data"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn fail(&self, msg: &str) -> String {
        format!("{} at byte {}", msg, self.i)
    }
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn lit(&mut self, w: &str) -> bool {
        if self.s[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            true
        } else {
            false
        }
    }
    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_PARSE_DEPTH {
            return Err(self.fail("JSON nested too deeply"));
        }
        let c = match self.peek() {
            None => return Err(self.fail("unexpected end of JSON")),
            Some(c) => c,
        };
        match c {
            b'{' => {
                self.i += 1;
                let mut o = Object::new();
                self.ws();
                if self.peek() == Some(b'}') {
                    self.i += 1;
                    return Ok(Value::Obj(o));
                }
                loop {
                    self.ws();
                    if self.peek() != Some(b'"') {
                        return Err(self.fail("expected a string key"));
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.peek() != Some(b':') {
                        return Err(self.fail("expected ':'"));
                    }
                    self.i += 1;
                    self.ws();
                    let v = self.value(depth + 1)?;
                    o.set(k, v);
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Value::Obj(o));
                        }
                        _ => return Err(self.fail("expected ',' or '}'")),
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut out = vec![];
                self.ws();
                if self.peek() == Some(b']') {
                    self.i += 1;
                    return Ok(Value::Arr(out));
                }
                loop {
                    self.ws();
                    out.push(self.value(depth + 1)?);
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Value::Arr(out));
                        }
                        _ => return Err(self.fail("expected ',' or ']'")),
                    }
                }
            }
            b'"' => Ok(Value::Str(self.string()?)),
            _ if self.lit("null") => Ok(Value::Null),
            _ if self.lit("true") => Ok(Value::Bool(true)),
            _ if self.lit("false") => Ok(Value::Bool(false)),
            _ if self.lit("NaN") => Ok(Value::Float(f64::NAN)),
            _ if self.lit("Infinity") => Ok(Value::Float(f64::INFINITY)),
            _ if self.lit("-Infinity") => Ok(Value::Float(f64::NEG_INFINITY)),
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(self.fail("unexpected character")),
        }
    }
    fn digits(&mut self) -> usize {
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
            self.i += 1;
        }
        self.i - start
    }
    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        if self.peek() == Some(b'0') {
            self.i += 1;
        } else if self.digits() == 0 {
            return Err(self.fail("bad number"));
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            let save = self.i;
            self.i += 1;
            if self.digits() == 0 {
                self.i = save;
            } else {
                is_float = true;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            let save = self.i;
            self.i += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.i += 1;
            }
            if self.digits() == 0 {
                self.i = save;
            } else {
                is_float = true;
            }
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).unwrap();
        if is_float {
            return text.parse::<f64>().map(Value::Float).map_err(|_| self.fail("bad number"));
        }
        if let Ok(n) = text.parse::<i64>() {
            return Ok(Value::Int(n));
        }
        let neg = text.starts_with('-');
        let t = text.trim_start_matches('-').trim_start_matches('0');
        Ok(Value::BigInt(if neg { format!("-{}", t) } else { t.to_string() }))
    }
    fn hex4(&self, at: usize) -> Option<u32> {
        let b = self.s.get(at..at + 4)?;
        u32::from_str_radix(std::str::from_utf8(b).ok()?, 16).ok()
    }
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = match self.peek() {
                None => return Err(self.fail("unterminated string")),
                Some(c) => c,
            };
            if c == b'"' {
                self.i += 1;
                return Ok(out);
            }
            if c < 0x20 {
                return Err(self.fail("invalid control character in string"));
            }
            if c != b'\\' {
                // copy a run of plain bytes (the input is valid UTF-8)
                let start = self.i;
                while self.i < self.s.len() && !matches!(self.s[self.i], b'"' | b'\\') && self.s[self.i] >= 0x20 {
                    self.i += 1;
                }
                out.push_str(std::str::from_utf8(&self.s[start..self.i]).unwrap());
                continue;
            }
            self.i += 1;
            let e = match self.peek() {
                None => return Err(self.fail("unterminated string")),
                Some(e) => e,
            };
            self.i += 1;
            match e {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{8}'),
                b'f' => out.push('\u{c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    let u = self.hex4(self.i).ok_or_else(|| self.fail("bad \\u escape"))?;
                    self.i += 4;
                    if (0xD800..=0xDBFF).contains(&u) {
                        if self.s.get(self.i) == Some(&b'\\') && self.s.get(self.i + 1) == Some(&b'u') {
                            if let Some(lo) = self.hex4(self.i + 2) {
                                if (0xDC00..=0xDFFF).contains(&lo) {
                                    self.i += 6;
                                    out.push(char::from_u32(0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00)).unwrap());
                                    continue;
                                }
                            }
                        }
                        out.push(lone_surrogate(u));
                    } else if (0xDC00..=0xDFFF).contains(&u) {
                        out.push(lone_surrogate(u));
                    } else {
                        out.push(char::from_u32(u).unwrap());
                    }
                }
                _ => return Err(self.fail("invalid escape")),
            }
        }
    }
}

// ---------------------------------------------------------------- canonical JSON

fn canonical_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c if is_lone_surrogate(c) => {
                let _ = write!(out, "\\u{:04x}", surrogate_unit(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The canonical JSON text (spec/SEMANTICS.md section 6): keys sorted by bytes and ASCII only, no
/// whitespace, integers only, and only `"`, `\` and characters below U+0020 escaped. Floats and non-ASCII
/// keys are an error. (An unpaired surrogate, which Python cannot encode as UTF-8 at all, is written as a
/// `\udXXX` escape.)
pub fn canonical(v: &Value) -> Result<String, String> {
    let mut out = String::new();
    canon(&mut out, v)?;
    Ok(out)
}

fn canon(out: &mut String, v: &Value) -> Result<(), String> {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::BigInt(s) => out.push_str(s),
        Value::Float(_) => return Err("canonical JSON does not encode float (floats are not allowed)".into()),
        Value::Str(s) => canonical_str(out, s),
        Value::Arr(a) => {
            out.push('[');
            for (i, e) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon(out, e)?;
            }
            out.push(']');
        }
        Value::Obj(o) => {
            if o.keys().iter().any(|k| !k.is_ascii()) {
                return Err("canonical JSON object keys must be ASCII strings".into());
            }
            let mut keys: Vec<&String> = o.keys().iter().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical_str(out, k);
                out.push(':');
                canon(out, o.at(k))?;
            }
            out.push('}');
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- Python's json.dumps

/// Python's `json.dumps(v, ensure_ascii=False[, sort_keys=True])`: `", "` and `": "` separators, only
/// `"`, `\`, \n, \r, \t, \b, \f and the other characters below U+0020 escaped, floats as `repr()`, NaN
/// and Infinity as Python writes them. Keys are sorted by code point when `sort_keys` is set.
pub fn py_dumps(v: &Value, sort_keys: bool) -> String {
    let mut out = String::new();
    dumps(&mut out, v, sort_keys);
    out
}

fn py_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c if is_lone_surrogate(c) => {
                let _ = write!(out, "\\u{:04x}", surrogate_unit(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn dumps(out: &mut String, v: &Value, sort_keys: bool) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::BigInt(s) => out.push_str(s),
        Value::Float(f) => {
            if f.is_nan() {
                out.push_str("NaN")
            } else if f.is_infinite() {
                out.push_str(if *f > 0.0 { "Infinity" } else { "-Infinity" })
            } else {
                out.push_str(&py_float(*f))
            }
        }
        Value::Str(s) => py_json_str(out, s),
        Value::Arr(a) => {
            out.push('[');
            for (i, e) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                dumps(out, e, sort_keys);
            }
            out.push(']');
        }
        Value::Obj(o) => {
            let mut keys: Vec<&String> = o.keys().iter().collect();
            if sort_keys {
                keys.sort(); // UTF-8 byte order is code point order
            }
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                py_json_str(out, k);
                out.push_str(": ");
                dumps(out, o.at(k), sort_keys);
            }
            out.push('}');
        }
    }
}

/// `repr(float)` for a finite float: the shortest text that reads back, in exponent form below 1e-4 and
/// from 1e16, always with `.0` or an exponent.
pub fn py_float(f: f64) -> String {
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let e = format!("{:e}", f); // shortest digits: d.ddde-X
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if (-4..16).contains(&exp) {
        let s = format!("{}", f);
        if s.contains('.') {
            s
        } else {
            s + ".0"
        }
    } else {
        format!("{}e{}{:02}", mant, if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

// ---------------------------------------------------------------- equality

/// JSON value equality as the conformance vectors require: key order does not matter, an integer
/// equals the same number as a float, and true is not 1.
pub fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::BigInt(x), Value::BigInt(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => (*x as f64) == *y,
        (Value::Arr(x), Value::Arr(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| equal(p, q)),
        (Value::Obj(x), Value::Obj(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).map(|w| equal(v, w)).unwrap_or(false))
        }
        _ => false,
    }
}
