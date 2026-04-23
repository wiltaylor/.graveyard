use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolOutput};

pub struct WriteTool;

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &'static str {
        "Write"
    }

    fn description(&self) -> &'static str {
        super::built_in_tool_doc("Write")
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to write. Parent directories are created if missing."
                },
                "content": {
                    "type": "string",
                    "description": "File contents to write. Overwrites existing files."
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let path = match input.get("file_path").and_then(Value::as_str) {
            Some(p) => p.to_string(),
            None => return ToolOutput::error("Write: `file_path` is required"),
        };
        let content = match input.get("content").and_then(Value::as_str) {
            Some(c) => c.to_string(),
            None => return ToolOutput::error("Write: `content` is required"),
        };

        if let Some(parent) = std::path::Path::new(&path).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    return ToolOutput::error(format!("Write: mkdir {}: {e}", parent.display()));
                }
            }
        }

        match tokio::fs::write(&path, content).await {
            Ok(()) => ToolOutput::ok(format!("wrote {path}")),
            Err(e) => ToolOutput::error(format!("Write: {path}: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn writes_file_and_creates_parents() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested/child/file.txt");
        let out = WriteTool
            .invoke(json!({
                "file_path": path.display().to_string(),
                "content": "hello\n"
            }))
            .await;
        assert!(!out.is_error, "{}", out.text);
        let back = std::fs::read_to_string(&path).unwrap();
        assert_eq!(back, "hello\n");
    }

    #[tokio::test]
    async fn missing_content_is_error() {
        let out = WriteTool.invoke(json!({"file_path": "/tmp/x"})).await;
        assert!(out.is_error);
    }
}
