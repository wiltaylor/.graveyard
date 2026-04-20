/// Build the system-generated preamble that is prepended to each agent's
/// instructions. Describes the `aciddog` CLI surface so the agent knows
/// which subcommands it can invoke.
///
/// `binary` is the absolute path to the running aciddog binary, so the
/// agent can invoke it directly without relying on it being on `PATH`.
///
/// Extending: add a new `push_section` call for each new subcommand just
/// before the `// add new subcommand sections above this line` marker.
pub fn cli_preamble(binary: &str) -> String {
    let mut out = String::new();
    push_line(&mut out, "## aciddog agent CLI");
    push_line(
        &mut out,
        "You have an `aciddog` command available that lets you interact with \
         the agent orchestrator running this session. Invoke it using the \
         absolute path below (it may not be on your `PATH`).",
    );
    push_line(&mut out, &format!("Binary path: `{binary}`"));
    push_line(&mut out, "### Subcommands");

    push_section(
        &mut out,
        binary,
        "test",
        "verify connectivity to the agent orchestrator.",
    );
    // add new subcommand sections above this line

    out
}

/// Append a line followed by a blank line, so the final rendered prompt has
/// visible breathing room between entries.
fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push_str("\n\n");
}

fn push_section(out: &mut String, binary: &str, name: &str, description: &str) {
    push_line(
        out,
        &format!("- `{binary} {name}` — {description}"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preamble_mentions_test_subcommand_with_binary_path() {
        let p = cli_preamble("/opt/aciddog/bin/aciddog");
        assert!(
            p.contains("/opt/aciddog/bin/aciddog test"),
            "missing fully-qualified `test` entry: {p}"
        );
        assert!(
            p.to_lowercase().contains("connectivity"),
            "missing connectivity sentence: {p}"
        );
    }

    #[test]
    fn preamble_has_header() {
        let p = cli_preamble("/usr/local/bin/aciddog");
        assert!(p.starts_with("## aciddog agent CLI"), "missing header: {p}");
    }

    #[test]
    fn preamble_surfaces_binary_path_explicitly() {
        let p = cli_preamble("/tmp/weird path/aciddog");
        assert!(
            p.contains("Binary path: `/tmp/weird path/aciddog`"),
            "binary path callout missing: {p}"
        );
    }
}
