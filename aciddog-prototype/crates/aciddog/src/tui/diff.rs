//! Unified diff generation for Edit / Write tool blocks.

use similar::TextDiff;

/// Produce a unified diff between `old` and `new` text for `file`.
pub fn unified(old: &str, new: &str, file: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let body = diff
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{file}"), &format!("b/{file}"))
        .to_string();

    if body.trim().is_empty() {
        format!("--- a/{file}\n+++ b/{file}\n@@ no changes @@\n")
    } else {
        body
    }
}

/// Build a synthetic "new file" unified diff for the Write tool.
pub fn synth_new_file(file: &str, content: &str) -> String {
    let line_count = content.lines().count();
    let mut out = String::new();
    out.push_str("--- /dev/null\n");
    out.push_str(&format!("+++ b/{file}\n"));
    out.push_str(&format!("@@ -0,0 +1,{line_count} @@\n"));
    for line in content.lines() {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_diff_contains_added_and_removed_lines() {
        let old = "alpha\nbeta\ngamma\n";
        let new = "alpha\nBETA\ngamma\n";
        let diff = unified(old, new, "test.txt");
        assert!(diff.contains("--- a/test.txt"));
        assert!(diff.contains("+++ b/test.txt"));
        assert!(diff.contains("-beta"));
        assert!(diff.contains("+BETA"));
    }

    #[test]
    fn unified_diff_on_identical_inputs_is_header_only() {
        let diff = unified("same\n", "same\n", "test.txt");
        assert!(diff.contains("--- a/test.txt"));
        assert!(diff.contains("no changes"));
    }

    #[test]
    fn synth_new_file_marks_additions() {
        let diff = synth_new_file("fresh.txt", "hello\nworld\n");
        assert!(diff.contains("--- /dev/null"));
        assert!(diff.contains("+++ b/fresh.txt"));
        assert!(diff.contains("@@ -0,0 +1,2 @@"));
        assert!(diff.contains("+hello"));
        assert!(diff.contains("+world"));
    }

    #[test]
    fn synth_new_file_handles_empty_content() {
        let diff = synth_new_file("empty.txt", "");
        assert!(diff.contains("@@ -0,0 +1,0 @@"));
    }
}
