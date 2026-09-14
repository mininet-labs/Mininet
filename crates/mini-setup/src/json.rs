//! A small JSON emitter for `--json`.
//!
//! Same envelope shape and the same reasoning as `mini-cli`'s emitter: this
//! binary only ever produces JSON and never parses it, so a few dozen lines
//! of plain Rust replace a dependency. Field *names* are not decided here ---
//! they come from `mini_windows_setup::report`, so this tool and
//! `mini windows-setup` describe the same facts identically.

use mini_windows_setup::Field;

/// `{"ok":true,"kind":"setup.install",...fields}`
pub fn ok(kind: &str, fields: &[(&'static str, Field)]) -> String {
    let mut out = String::from("{\"ok\":true,\"kind\":");
    push_string(&mut out, kind);
    for (name, value) in fields {
        out.push(',');
        push_string(&mut out, name);
        out.push(':');
        push_field(&mut out, value);
    }
    out.push('}');
    out
}

/// `{"ok":false,"kind":...,"error_code":...,"message":...}`
pub fn err(kind: &str, error_code: &str, message: &str) -> String {
    let mut out = String::from("{\"ok\":false,\"kind\":");
    push_string(&mut out, kind);
    out.push_str(",\"error_code\":");
    push_string(&mut out, error_code);
    out.push_str(",\"message\":");
    push_string(&mut out, message);
    out.push('}');
    out
}

fn push_field(out: &mut String, value: &Field) {
    match value {
        Field::Text(text) => push_string(out, text),
        Field::MaybeText(Some(text)) => push_string(out, text),
        Field::MaybeText(None) => out.push_str("null"),
        Field::Number(number) => out.push_str(&number.to_string()),
        Field::Flag(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Field::List(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                push_string(out, item);
            }
            out.push(']');
        }
    }
}

fn push_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ok_envelope_is_one_line_and_carries_every_field_type() {
        let line = ok(
            "setup.install",
            &[
                ("version", Field::Text("0.1.0".to_string())),
                ("files_written", Field::Number(3)),
                ("intact", Field::Flag(true)),
                ("previous_version", Field::MaybeText(None)),
                (
                    "shell_actions",
                    Field::List(vec!["create-shortcut:C:\\a".to_string()]),
                ),
            ],
        );
        assert_eq!(
            line,
            r#"{"ok":true,"kind":"setup.install","version":"0.1.0","files_written":3,"intact":true,"previous_version":null,"shell_actions":["create-shortcut:C:\\a"]}"#
        );
        assert!(!line.contains('\n'));
    }

    #[test]
    fn an_error_envelope_carries_a_stable_machine_code() {
        let line = err("setup.install", "digest_mismatch", "mini.exe did not match");
        assert_eq!(
            line,
            r#"{"ok":false,"kind":"setup.install","error_code":"digest_mismatch","message":"mini.exe did not match"}"#
        );
    }

    #[test]
    fn control_characters_and_quotes_in_a_message_are_escaped() {
        let line = err("setup.verify", "io", "weird \"path\"\nwith\ttabs\u{1}");
        assert!(line.contains(r#"weird \"path\"\nwith\ttabs\u0001"#));
        assert!(!line.contains('\n'));
    }
}
