use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolOutput};

pub struct EditTool;

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &'static str {
        "Edit"
    }

    fn description(&self) -> &'static str {
        super::built_in_tool_doc("Edit")
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit."
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact string to replace. Must be unique in the file unless `replace_all` is true."
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement text."
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "When true, replace every occurrence; otherwise `old_string` must be unique.",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let path = match input.get("file_path").and_then(Value::as_str) {
            Some(p) => p.to_string(),
            None => return ToolOutput::error("Edit: `file_path` is required"),
        };
        let old = match input.get("old_string").and_then(Value::as_str) {
            Some(s) => s,
            None => return ToolOutput::error("Edit: `old_string` is required"),
        };
        let new = match input.get("new_string").and_then(Value::as_str) {
            Some(s) => s,
            None => return ToolOutput::error("Edit: `new_string` is required"),
        };
        let replace_all = input
            .get("replace_all")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let original = match tokio::fs::read_to_string(&path).await {
            Ok(s) => s,
            Err(e) => return ToolOutput::error(format!("Edit: read {path}: {e}")),
        };

        let matches = original.matches(old).count();
        if matches == 0 {
            return ToolOutput::error(format!(
                "Edit: `old_string` not found in {path}. Read the file and use an exact snippet."
            ));
        }
        if !replace_all && matches > 1 {
            return ToolOutput::error(format!(
                "Edit: `old_string` matches {matches} times in {path}. Provide a longer, unique snippet or set `replace_all=true`."
            ));
        }

        let updated = if replace_all {
            original.replace(old, new)
        } else {
            original.replacen(old, new, 1)
        };

        match tokio::fs::write(&path, updated).await {
            Ok(()) => ToolOutput::ok(format!(
                "edited {path} ({} replacement{})",
                matches.min(if replace_all { matches } else { 1 }),
                if matches == 1 || !replace_all { "" } else { "s" }
            )),
            Err(e) => ToolOutput::error(format!("Edit: write {path}: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    async fn write_file(contents: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut f, contents.as_bytes()).unwrap();
        f
    }

    #[tokio::test]
    async fn replaces_unique_match() {
        let f = write_file("alpha beta gamma").await;
        let out = EditTool
            .invoke(json!({
                "file_path": f.path().display().to_string(),
                "old_string": "beta",
                "new_string": "DELTA"
            }))
            .await;
        assert!(!out.is_error, "{}", out.text);
        let back = std::fs::read_to_string(f.path()).unwrap();
        assert_eq!(back, "alpha DELTA gamma");
    }

    #[tokio::test]
    async fn rejects_duplicate_without_replace_all() {
        let f = write_file("x x").await;
        let out = EditTool
            .invoke(json!({
                "file_path": f.path().display().to_string(),
                "old_string": "x",
                "new_string": "y"
            }))
            .await;
        assert!(out.is_error);
    }

    #[tokio::test]
    async fn replace_all_rewrites_every_occurrence() {
        let f = write_file("x x x").await;
        let out = EditTool
            .invoke(json!({
                "file_path": f.path().display().to_string(),
                "old_string": "x",
                "new_string": "y",
                "replace_all": true
            }))
            .await;
        assert!(!out.is_error, "{}", out.text);
        let back = std::fs::read_to_string(f.path()).unwrap();
        assert_eq!(back, "y y y");
    }

    #[tokio::test]
    async fn missing_match_is_error() {
        let f = write_file("nope").await;
        let out = EditTool
            .invoke(json!({
                "file_path": f.path().display().to_string(),
                "old_string": "zzz",
                "new_string": "yyy"
            }))
            .await;
        assert!(out.is_error);
    }
}
