//! A minimal, hand-rolled JSON *emitter* (no parser -- this crate only
//! ever produces JSON, never consumes its own output) for `--json`
//! command output. No `serde`/`serde_json` dependency: matches this
//! workspace's established convention (`mini-forge`'s git-object framing,
//! `mini-installer`'s event log) of hand-rolled encoding over pulling in a
//! dependency where a few dozen lines of plain Rust do the job, and this
//! crate's own `cli.rs` module docs state that reasoning explicitly for
//! argument parsing already.

/// A JSON value this crate can build and render. No `Number` variant
/// backed by `f64` -- every number this crate ever emits (fuel, byte
/// counts, sequence numbers, attester counts) is an exact non-negative
/// integer, so `Number` stores its already-formatted decimal digits and
/// there is no float-precision surprise to worry about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub fn str(s: impl Into<String>) -> Self {
        JsonValue::String(s.into())
    }

    pub fn num(n: u64) -> Self {
        JsonValue::Number(n.to_string())
    }

    pub fn opt_str(s: Option<impl Into<String>>) -> Self {
        match s {
            Some(s) => JsonValue::String(s.into()),
            None => JsonValue::Null,
        }
    }

    pub fn strs(items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        JsonValue::Array(items.into_iter().map(|s| JsonValue::str(s)).collect())
    }

    /// Render this value as a single-line JSON document (no pretty
    /// printing -- machine consumption is the only intended reader).
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            JsonValue::Null => out.push_str("null"),
            JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            JsonValue::Number(n) => out.push_str(n),
            JsonValue::String(s) => {
                out.push('"');
                escape_into(s, out);
                out.push('"');
            }
            JsonValue::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            JsonValue::Object(fields) => {
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('"');
                    escape_into(k, out);
                    out.push('"');
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

fn escape_into(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

/// `{"ok": true, "kind": "<verb.noun>", ...fields}`
pub fn ok_envelope(kind: &str, fields: Vec<(&str, JsonValue)>) -> String {
    let mut all = vec![
        ("ok".to_string(), JsonValue::Bool(true)),
        ("kind".to_string(), JsonValue::str(kind)),
    ];
    all.extend(fields.into_iter().map(|(k, v)| (k.to_string(), v)));
    JsonValue::Object(all).render()
}

/// `{"ok": false, "kind": "<verb.noun>", "error_code": "...", "message": "..."}`
pub fn err_envelope(kind: &str, error_code: &str, message: &str) -> String {
    JsonValue::Object(vec![
        ("ok".to_string(), JsonValue::Bool(false)),
        ("kind".to_string(), JsonValue::str(kind)),
        ("error_code".to_string(), JsonValue::str(error_code)),
        ("message".to_string(), JsonValue::str(message)),
    ])
    .render()
}

/// The output of one command: the existing human-readable text (unchanged
/// default), plus, for commands that create or inspect a specific object,
/// the structured fields a `--json` caller needs to chain into a later
/// command without scraping text (an `ObjectId`, a version string, an
/// attester count -- never the human sentence itself re-parsed).
#[derive(Debug, Clone, Default)]
pub struct CommandResult {
    pub human: String,
    pub fields: Vec<(&'static str, JsonValue)>,
}

impl CommandResult {
    pub fn new(human: impl Into<String>) -> Self {
        CommandResult {
            human: human.into(),
            fields: Vec::new(),
        }
    }

    pub fn field(mut self, key: &'static str, value: JsonValue) -> Self {
        self.fields.push((key, value));
        self
    }

    pub fn render(self, json: bool, kind: &str) -> String {
        if json {
            ok_envelope(kind, self.fields)
        } else {
            self.human
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped() {
        let v = JsonValue::str("line1\nline2\t\"quoted\"\\backslash");
        assert_eq!(v.render(), r#""line1\nline2\t\"quoted\"\\backslash""#);
    }

    #[test]
    fn ok_envelope_shape() {
        let out = ok_envelope(
            "release.create",
            vec![
                ("release_id", JsonValue::str("abc123")),
                ("attesters", JsonValue::num(2)),
            ],
        );
        assert_eq!(
            out,
            r#"{"ok":true,"kind":"release.create","release_id":"abc123","attesters":2}"#
        );
    }

    #[test]
    fn err_envelope_shape() {
        let out = err_envelope("release.verify", "forge", "not enough attestations");
        assert_eq!(
            out,
            r#"{"ok":false,"kind":"release.verify","error_code":"forge","message":"not enough attestations"}"#
        );
    }

    #[test]
    fn null_and_array_render() {
        assert_eq!(JsonValue::opt_str(None::<String>).render(), "null");
        assert_eq!(JsonValue::strs(["a", "b"]).render(), r#"["a","b"]"#);
    }
}
