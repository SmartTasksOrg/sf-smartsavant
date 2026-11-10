//! The Python behaviour the reference relies on: code-point lengths and slices, `str()`, `repr()`, truth
//! values, `str.lower()`, `str.strip(chars)` and iteration over a value.
use crate::json::{is_lone_surrogate, py_float, Value};

/// `len(s)` in code points.
pub fn len(s: &str) -> usize {
    s.chars().count()
}

/// `s[:n]` in code points.
pub fn head(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

/// `str.lower()` (Rust's full Unicode lowercase mapping, as Python's).
pub fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// `str.strip(chars)`.
pub fn strip(s: &str, chars: &str) -> String {
    s.trim_matches(|c| chars.contains(c)).to_string()
}

/// `bool(v)`.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::BigInt(_) => true,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Arr(a) => !a.is_empty(),
        Value::Obj(o) => !o.is_empty(),
    }
}

/// Iteration as Python does it: a list gives its items, a string its characters, a dict its keys;
/// anything else (where Python would raise) gives nothing.
pub fn iter(v: &Value) -> Vec<Value> {
    match v {
        Value::Arr(a) => a.clone(),
        Value::Str(s) => s.chars().map(|c| Value::Str(c.to_string())).collect(),
        Value::Obj(o) => o.keys().iter().map(|k| Value::Str(k.clone())).collect(),
        _ => vec![],
    }
}

/// The Python type name of a value.
pub fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Int(_) | Value::BigInt(_) => "int",
        Value::Float(_) => "float",
        Value::Str(_) => "str",
        Value::Arr(_) => "list",
        Value::Obj(_) => "dict",
    }
}

/// `str(v)`: a string as is, anything else as `repr()`.
pub fn to_str(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        _ => repr(v),
    }
}

/// `repr()` of a JSON value.
pub fn repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(n) => n.to_string(),
        Value::BigInt(s) => s.clone(),
        Value::Float(f) => {
            if f.is_nan() {
                "nan".into()
            } else if f.is_infinite() {
                if *f > 0.0 {
                    "inf".into()
                } else {
                    "-inf".into()
                }
            } else {
                py_float(*f)
            }
        }
        Value::Str(s) => repr_str(s),
        Value::Arr(a) => format!("[{}]", a.iter().map(repr).collect::<Vec<_>>().join(", ")),
        Value::Obj(o) => format!(
            "{{{}}}",
            o.iter().map(|(k, v)| format!("{}: {}", repr_str(k), repr(v))).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Characters `str.isprintable()` is false for, as far as this crate knows without Unicode category
/// tables: controls (Cc), the separators other than U+0020 (Zs, Zl, Zp), the format characters (Cf) and
/// private-use characters (Co). Unassigned code points (Cn) are taken as printable.
fn unprintable(c: char) -> bool {
    let n = c as u32;
    matches!(n,
        0x00..=0x1F | 0x7F..=0xA0 | 0xAD | 0x600..=0x605 | 0x61C | 0x6DD | 0x70F | 0x890..=0x891 | 0x8E2
        | 0x1680 | 0x180E | 0x2000..=0x200F | 0x2028..=0x202F | 0x205F..=0x2064 | 0x2066..=0x206F | 0x3000
        | 0xD800..=0xDFFF | 0xE000..=0xF8FF | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F
        | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F | 0xF0000..=0xFFFFD
        | 0x100000..=0x10FFFD)
}

/// `repr(str)`: the quote Python picks, and non-printable characters escaped.
pub fn repr_str(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::new();
    out.push(q);
    for c in s.chars() {
        if c == q || c == '\\' {
            out.push('\\');
            out.push(c);
        } else if c == '\n' {
            out.push_str("\\n");
        } else if c == '\r' {
            out.push_str("\\r");
        } else if c == '\t' {
            out.push_str("\\t");
        } else if is_lone_surrogate(c) {
            out.push_str(&format!("\\u{:04x}", c as u32 - 0x10F800 + 0xD800));
        } else if c != ' ' && unprintable(c) {
            let n = c as u32;
            if n < 0x100 {
                out.push_str(&format!("\\x{:02x}", n));
            } else if n < 0x10000 {
                out.push_str(&format!("\\u{:04x}", n));
            } else {
                out.push_str(&format!("\\U{:08x}", n));
            }
        } else {
            out.push(c);
        }
    }
    out.push(q);
    out
}
