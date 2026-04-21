//! Short, one-line descriptions for the built-in tools, shared by all
//! cores that enumerate tool surfaces in the first-turn `<tools>`
//! preamble. Lives outside the per-tool modules so the `claude_code`
//! core (which never constructs a `ToolRegistry`) can pull the same
//! strings without reaching into each tool's impl.

/// Short description used by first-turn `<tools>` preambles. Unknown
/// names get a generic line so orchestrator additions don't break the
/// surface even when this lookup table lags.
pub fn built_in_tool_doc(name: &str) -> &'static str {
    match name {
        "Bash" => "execute a shell command. Output is returned as text. Use absolute paths and quote spaces.",
        "Read" => "read a file from disk by absolute path. Returns the file contents.",
        "Edit" => "perform an exact-string replacement in a file. The `old_string` must be unique unless you set `replace_all`.",
        "Write" => "write a file at an absolute path. Overwrites existing files; create only what's needed.",
        "Grep" => "ripgrep-backed content search. Supports regex; filter by glob or file type.",
        "Glob" => "fast file-pattern matching (e.g. `**/*.rs`). Use to discover files by name.",
        "WebFetch" => "fetch a URL and return processed text content.",
        "WebSearch" => "search the web and return result snippets.",
        _ => "enabled",
    }
}
