//! Runtime variable values. One enum covers every `PropertyKind`
//! (including `Hidden`, which stores as `Text`). The engine stores
//! values in a `HashMap<String, PropertyValue>` shared with the
//! per-instance `workflow_get` / `workflow_set` tools.

use crate::config::PropertyKind;

/// A live workflow variable. `Hidden` properties store as `Text`
/// because they're surfaced to agents as plain strings.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Text(String),
    Number(f64),
    Selection(String),
    Multi(Vec<String>),
}

impl PropertyValue {
    /// Seed from a `PropertyKind`'s declared default. `None` means the
    /// property has no default and should be absent from the map until
    /// explicitly set (by user input or `workflow_set`).
    pub fn default_from(kind: &PropertyKind) -> Option<Self> {
        match kind {
            PropertyKind::Text { default } => default.clone().map(PropertyValue::Text),
            PropertyKind::Number { default, .. } => (*default).map(PropertyValue::Number),
            PropertyKind::Select { default, .. } => default.clone().map(PropertyValue::Selection),
            PropertyKind::MultiSelect { default, .. } => default.clone().map(PropertyValue::Multi),
            PropertyKind::Hidden { default } => default.clone().map(PropertyValue::Text),
        }
    }

    /// String form suitable for `$$VAR$$` substitution and for the
    /// TUI's variables panel. `MultiSelect` joins with `", "`.
    pub fn as_display(&self) -> String {
        match self {
            PropertyValue::Text(s) | PropertyValue::Selection(s) => s.clone(),
            PropertyValue::Number(n) => {
                // Strip trailing `.0` on whole numbers so `1` → "1" not
                // "1.0" (less jarring in form defaults).
                if n.fract() == 0.0 && n.is_finite() {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
            PropertyValue::Multi(items) => items.join(", "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_from_text() {
        let kind = PropertyKind::Text {
            default: Some("hi".into()),
        };
        let v = PropertyValue::default_from(&kind).unwrap();
        assert_eq!(v, PropertyValue::Text("hi".into()));
    }

    #[test]
    fn default_from_hidden_stores_as_text() {
        let kind = PropertyKind::Hidden {
            default: Some("s3cr3t".into()),
        };
        let v = PropertyValue::default_from(&kind).unwrap();
        assert_eq!(v, PropertyValue::Text("s3cr3t".into()));
    }

    #[test]
    fn default_from_none_returns_none() {
        let kind = PropertyKind::Text { default: None };
        assert!(PropertyValue::default_from(&kind).is_none());
    }

    #[test]
    fn display_joins_multi_select() {
        let v = PropertyValue::Multi(vec!["a".into(), "b".into()]);
        assert_eq!(v.as_display(), "a, b");
    }

    #[test]
    fn display_strips_trailing_zero_on_whole_numbers() {
        assert_eq!(PropertyValue::Number(3.0).as_display(), "3");
        assert_eq!(PropertyValue::Number(3.5).as_display(), "3.5");
    }
}
