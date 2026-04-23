//! Best-effort lookup from a model id string to its maximum context
//! window in tokens. The TUI uses this to render a "used / total"
//! indicator on the AgentInfo block.
//!
//! This is intentionally a static table rather than a remote query —
//! aciddog ships a single binary and we don't want a network round-trip
//! just to draw a status line. If a model isn't recognised we return
//! `None` and the UI falls back to showing only the raw used count.

/// Return the maximum context window in tokens for the given model id,
/// or `None` if the model is unknown. The match is case-insensitive on
/// the prefix and tolerates the trailing `[1m]` capability suffix used
/// by claude long-context variants.
pub fn model_context_window(model: &str) -> Option<u32> {
    let m = model.trim().to_ascii_lowercase();
    if m.is_empty() {
        return None;
    }

    // Anthropic long-context capability suffix overrides everything
    // else (e.g. `claude-opus-4-7[1m]`).
    if m.ends_with("[1m]") {
        return Some(1_000_000);
    }

    // Anthropic Claude family. All current 3.x/4.x models default to
    // 200K unless the [1m] suffix bumps them above.
    if m.starts_with("claude-")
        || m.starts_with("anthropic/claude-")
        || m.starts_with("anthropic.claude-")
    {
        return Some(200_000);
    }

    // OpenAI families.
    if m.starts_with("gpt-4o") || m.starts_with("openai/gpt-4o") {
        return Some(128_000);
    }
    if m.starts_with("gpt-4-turbo") || m.starts_with("openai/gpt-4-turbo") {
        return Some(128_000);
    }
    if m.starts_with("gpt-4.1") || m.starts_with("openai/gpt-4.1") {
        return Some(1_000_000);
    }
    if m.starts_with("gpt-5") || m.starts_with("openai/gpt-5") {
        return Some(400_000);
    }
    if m.starts_with("gpt-4") || m.starts_with("openai/gpt-4") {
        return Some(8_192);
    }
    if m.starts_with("gpt-3.5") || m.starts_with("openai/gpt-3.5") {
        return Some(16_385);
    }
    if m.starts_with("o1") || m.starts_with("openai/o1") {
        return Some(200_000);
    }
    if m.starts_with("o3") || m.starts_with("openai/o3") {
        return Some(200_000);
    }
    if m.starts_with("o4") || m.starts_with("openai/o4") {
        return Some(200_000);
    }

    // Google Gemini family.
    if m.starts_with("gemini-2.5") || m.starts_with("auto-gemini") {
        return Some(1_000_000);
    }
    if m.starts_with("gemini-1.5") || m.starts_with("gemini-2.0") {
        return Some(1_000_000);
    }
    if m.starts_with("gemini-") {
        return Some(32_768);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_default_is_200k() {
        assert_eq!(model_context_window("claude-opus-4-7"), Some(200_000));
        assert_eq!(model_context_window("claude-sonnet-4-6"), Some(200_000));
    }

    #[test]
    fn one_m_suffix_bumps_to_1m() {
        assert_eq!(model_context_window("claude-opus-4-7[1m]"), Some(1_000_000));
        assert_eq!(
            model_context_window("claude-sonnet-4-5[1m]"),
            Some(1_000_000)
        );
    }

    #[test]
    fn gpt_4o_is_128k() {
        assert_eq!(model_context_window("gpt-4o"), Some(128_000));
        assert_eq!(model_context_window("gpt-4o-mini"), Some(128_000));
        assert_eq!(model_context_window("openai/gpt-4o-mini"), Some(128_000));
    }

    #[test]
    fn gemini_2_5_is_1m() {
        assert_eq!(
            model_context_window("gemini-2.5-flash-lite"),
            Some(1_000_000)
        );
    }

    #[test]
    fn unknown_model_returns_none() {
        assert_eq!(model_context_window("totally-made-up-model"), None);
        assert_eq!(model_context_window(""), None);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(model_context_window("CLAUDE-OPUS-4-7"), Some(200_000));
        assert_eq!(model_context_window("Claude-Opus-4-7[1M]"), Some(1_000_000));
    }
}
