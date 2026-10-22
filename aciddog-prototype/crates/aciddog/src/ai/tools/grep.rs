use async_trait::async_trait;
use grep_regex::RegexMatcher;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use serde_json::{json, Value};

use super::{Tool, ToolOutput};

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &'static str {
        "Grep"
    }

    fn description(&self) -> &'static str {
        super::built_in_tool_doc("Grep")
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression to search for."
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search. Defaults to the current working directory."
                },
                "glob": {
                    "type": "string",
                    "description": "Optional glob filter (e.g. `**/*.rs`) restricting which files are searched."
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Match case-insensitively.",
                    "default": false
                },
                "max_matches": {
                    "type": "integer",
                    "description": "Cap the total number of matches returned. Default 200.",
                    "minimum": 1
                }
            },
            "required": ["pattern"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let pattern = match input.get("pattern").and_then(Value::as_str) {
            Some(p) => p.to_string(),
            None => return ToolOutput::error("Grep: `pattern` is required"),
        };
        let root = input
            .get("path")
            .and_then(Value::as_str)
            .map(String::from)
            .unwrap_or_else(|| ".".to_string());
        let glob_filter = input.get("glob").and_then(Value::as_str).map(String::from);
        let case_insensitive = input
            .get("case_insensitive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let max_matches = input
            .get("max_matches")
            .and_then(Value::as_u64)
            .unwrap_or(200) as usize;

        // Build the matcher. `grep-regex` accepts a case-insensitive
        // flag only via its own builder config.
        let matcher = match grep_regex::RegexMatcherBuilder::new()
            .case_insensitive(case_insensitive)
            .build(&pattern)
        {
            Ok(m) => m,
            Err(e) => return ToolOutput::error(format!("Grep: bad regex: {e}")),
        };

        // Offload filesystem+CPU search to a blocking task so tokio
        // threads aren't blocked.
        let root_clone = root.clone();
        let pattern_for_task = matcher.clone();
        let res = tokio::task::spawn_blocking(move || {
            search_tree(root_clone, pattern_for_task, glob_filter, max_matches)
        })
        .await;

        match res {
            Ok(Ok(out)) => {
                if out.is_empty() {
                    ToolOutput::ok("(no matches)".to_string())
                } else {
                    ToolOutput::ok(out)
                }
            }
            Ok(Err(e)) => ToolOutput::error(format!("Grep: {e}")),
            Err(e) => ToolOutput::error(format!("Grep: worker panicked: {e}")),
        }
    }
}

fn search_tree(
    root: String,
    matcher: RegexMatcher,
    glob_filter: Option<String>,
    max_matches: usize,
) -> Result<String, String> {
    let root_path = std::path::PathBuf::from(&root);
    let glob_matcher = match glob_filter.as_deref() {
        Some(g) => Some(glob::Pattern::new(g).map_err(|e| format!("bad glob: {e}"))?),
        None => None,
    };

    let mut hits: Vec<String> = Vec::new();
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(true)
        .build();

    let mut stack: Vec<std::path::PathBuf> = vec![root_path.clone()];
    while let Some(current) = stack.pop() {
        let meta = match std::fs::metadata(&current) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            let Ok(rd) = std::fs::read_dir(&current) else {
                continue;
            };
            for entry in rd.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Skip common noise directories so we don't descend
                // into build output or VCS internals.
                if current == root_path
                    && matches!(name_str.as_ref(), ".git" | "target" | "node_modules")
                {
                    continue;
                }
                stack.push(entry.path());
            }
            continue;
        }

        if let Some(filter) = &glob_matcher {
            if !filter.matches_path(&current) {
                continue;
            }
        }

        let path_string = current.display().to_string();
        let capped = hits.len();
        let budget = max_matches.saturating_sub(capped);
        if budget == 0 {
            break;
        }
        let mut local: Vec<String> = Vec::new();
        let search_res = searcher.search_path(
            &matcher,
            &current,
            UTF8(|lineno, line| {
                if local.len() >= budget {
                    return Ok(false);
                }
                local.push(format!("{path_string}:{lineno}:{}", line.trim_end()));
                Ok(true)
            }),
        );
        if search_res.is_err() {
            // Binary/unreadable files just get skipped.
            continue;
        }
        hits.extend(local);
        if hits.len() >= max_matches {
            break;
        }
    }

    Ok(hits.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn finds_regex_matches() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello world\nfoo bar\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "no match here\n").unwrap();

        let out = GrepTool
            .invoke(json!({
                "pattern": "hello",
                "path": dir.path().display().to_string()
            }))
            .await;
        assert!(!out.is_error, "{}", out.text);
        assert!(out.text.contains("hello world"), "got: {}", out.text);
    }

    #[tokio::test]
    async fn no_matches_yields_placeholder() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "nothing interesting\n").unwrap();
        let out = GrepTool
            .invoke(json!({
                "pattern": "zzzzzzz",
                "path": dir.path().display().to_string()
            }))
            .await;
        assert!(!out.is_error);
        assert!(out.text.contains("no matches"));
    }

    #[tokio::test]
    async fn case_insensitive_flag_works() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "Hello World\n").unwrap();
        let out = GrepTool
            .invoke(json!({
                "pattern": "hello",
                "path": dir.path().display().to_string(),
                "case_insensitive": true
            }))
            .await;
        assert!(!out.is_error);
        assert!(out.text.contains("Hello World"));
    }
}
