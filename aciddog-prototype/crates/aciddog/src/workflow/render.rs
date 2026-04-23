//! `$$VAR$$` substitution. Pure function so it's trivial to unit-test
//! and to call from the `Command` / `Worker` / `Conditional` step
//! executors.

use std::collections::HashMap;

use super::value::PropertyValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError {
    pub variable: String,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown workflow variable `{}`", self.variable)
    }
}

impl std::error::Error for RenderError {}

/// Replace every `$$NAME$$` token with the value from `vars`.
/// Unknown names return `RenderError`. The variable-name character
/// class matches identifier syntax: `[A-Za-z_][A-Za-z0-9_]*`.
pub fn render_template(
    template: &str,
    vars: &HashMap<String, PropertyValue>,
) -> Result<String, RenderError> {
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len());
    let mut i = 0;
    while i < bytes.len() {
        // Look for the opening `$$`.
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'$' {
            let name_start = i + 2;
            // First char of the name must be a letter or underscore,
            // otherwise the `$$` is a literal — emit both chars.
            if name_start < bytes.len() && is_ident_start(bytes[name_start]) {
                // Scan the name.
                let mut j = name_start + 1;
                while j < bytes.len() && is_ident_continue(bytes[j]) {
                    j += 1;
                }
                // Require the closing `$$`.
                if j + 1 < bytes.len() && bytes[j] == b'$' && bytes[j + 1] == b'$' {
                    let name = &template[name_start..j];
                    let Some(val) = vars.get(name) else {
                        return Err(RenderError {
                            variable: name.to_string(),
                        });
                    };
                    out.push_str(&val.as_display());
                    i = j + 2;
                    continue;
                }
            }
        }
        // Regular byte — append and advance. UTF-8 continuation bytes
        // travel with their lead byte since `$` is ASCII and won't
        // match against them.
        out.push(bytes[i] as char);
        i += 1;
    }
    Ok(out)
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> HashMap<String, PropertyValue> {
        let mut m = HashMap::new();
        m.insert("branch".into(), PropertyValue::Text("main".into()));
        m.insert(
            "suites".into(),
            PropertyValue::Multi(vec!["unit".into(), "integ".into()]),
        );
        m.insert("retries".into(), PropertyValue::Number(3.0));
        m
    }

    #[test]
    fn substitutes_single_var() {
        let got = render_template("on $$branch$$ ok", &vars()).unwrap();
        assert_eq!(got, "on main ok");
    }

    #[test]
    fn joins_multi_select_values() {
        let got = render_template("run $$suites$$", &vars()).unwrap();
        assert_eq!(got, "run unit, integ");
    }

    #[test]
    fn renders_numbers_without_trailing_zero() {
        let got = render_template("n=$$retries$$", &vars()).unwrap();
        assert_eq!(got, "n=3");
    }

    #[test]
    fn unknown_variable_errors() {
        let err = render_template("$$ghost$$", &vars()).unwrap_err();
        assert_eq!(err.variable, "ghost");
    }

    #[test]
    fn literal_dollar_signs_are_preserved() {
        let got = render_template("cost is $5$ usd", &vars()).unwrap();
        assert_eq!(got, "cost is $5$ usd");
    }

    #[test]
    fn unclosed_token_is_literal() {
        // No closing `$$` — treat as a literal run of chars.
        let got = render_template("$$branch", &vars()).unwrap();
        assert_eq!(got, "$$branch");
    }

    #[test]
    fn adjacent_substitutions() {
        let got = render_template("$$branch$$/$$branch$$", &vars()).unwrap();
        assert_eq!(got, "main/main");
    }
}
