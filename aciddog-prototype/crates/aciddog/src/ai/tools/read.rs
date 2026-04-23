use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolOutput};

pub struct ReadTool;

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &'static str {
        "Read"
    }

    fn description(&self) -> &'static str {
        super::built_in_tool_doc("Read")
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to read."
                },
                "offset": {
                    "type": "integer",
                    "description": "Optional 1-indexed line number to start reading from.",
                    "minimum": 1
                },
                "limit": {
                    "type": "integer",
                    "description": "Optional maximum number of lines to return.",
                    "minimum": 1
                }
            },
            "required": ["file_path"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let path = match input.get("file_path").and_then(Value::as_str) {
            Some(p) => p.to_string(),
            None => return ToolOutput::error("Read: `file_path` is required"),
        };
        let offset = input
            .get("offset")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);

        let contents = match tokio::fs::read_to_string(&path).await {
            Ok(s) => s,
            Err(e) => return ToolOutput::error(format!("Read: {path}: {e}")),
        };

        match (offset, limit) {
            (None, None) => ToolOutput::ok(contents),
            _ => {
                let start = offset.unwrap_or(1).saturating_sub(1);
                let out: String = contents
                    .lines()
                    .skip(start)
                    .take(limit.unwrap_or(usize::MAX))
                    .collect::<Vec<_>>()
                    .join("\n");
                ToolOutput::ok(out)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn reads_whole_file() {
        let mut f = NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut f, b"alpha\nbeta\ngamma\n").unwrap();
        let out = ReadTool
            .invoke(json!({"file_path": f.path().display().to_string()}))
            .await;
        assert!(!out.is_error);
        assert!(out.text.contains("alpha"));
        assert!(out.text.contains("gamma"));
    }

    #[tokio::test]
    async fn missing_file_is_error() {
        let out = ReadTool.invoke(json!({"file_path": "/no/such/file"})).await;
        assert!(out.is_error);
    }

    #[tokio::test]
    async fn offset_and_limit_slice_lines() {
        let mut f = NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut f, b"a\nb\nc\nd\n").unwrap();
        let out = ReadTool
            .invoke(json!({
                "file_path": f.path().display().to_string(),
                "offset": 2,
                "limit": 2
            }))
            .await;
        assert!(!out.is_error);
        assert_eq!(out.text, "b\nc");
    }
}
