use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolOutput};

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &'static str {
        "Glob"
    }

    fn description(&self) -> &'static str {
        super::built_in_tool_doc("Glob")
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern, e.g. `**/*.rs` or `src/ai/*.rs`."
                },
                "path": {
                    "type": "string",
                    "description": "Optional directory to anchor the pattern in. Defaults to the current working directory."
                }
            },
            "required": ["pattern"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let pattern = match input.get("pattern").and_then(Value::as_str) {
            Some(p) => p.to_string(),
            None => return ToolOutput::error("Glob: `pattern` is required"),
        };

        let full_pattern = match input.get("path").and_then(Value::as_str) {
            Some(dir) => {
                let mut p = std::path::PathBuf::from(dir);
                p.push(&pattern);
                p.display().to_string()
            }
            None => pattern,
        };

        let matches = match glob::glob(&full_pattern) {
            Ok(iter) => iter,
            Err(e) => return ToolOutput::error(format!("Glob: bad pattern: {e}")),
        };

        let mut rows: Vec<(std::path::PathBuf, std::time::SystemTime)> = Vec::new();
        for p in matches.flatten() {
            let mtime = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            rows.push((p, mtime));
        }
        rows.sort_by(|a, b| b.1.cmp(&a.1));

        if rows.is_empty() {
            return ToolOutput::ok(String::from("(no matches)"));
        }
        let out = rows
            .into_iter()
            .map(|(p, _)| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        ToolOutput::ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn finds_matches() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        std::fs::write(dir.path().join("c.txt"), "").unwrap();

        let out = GlobTool
            .invoke(json!({
                "pattern": "*.rs",
                "path": dir.path().display().to_string()
            }))
            .await;
        assert!(!out.is_error, "{}", out.text);
        assert!(out.text.contains("a.rs"));
        assert!(out.text.contains("b.rs"));
        assert!(!out.text.contains("c.txt"));
    }

    #[tokio::test]
    async fn empty_result_returns_placeholder() {
        let dir = TempDir::new().unwrap();
        let out = GlobTool
            .invoke(json!({
                "pattern": "*.never",
                "path": dir.path().display().to_string()
            }))
            .await;
        assert!(!out.is_error);
        assert!(out.text.contains("no matches"));
    }
}
