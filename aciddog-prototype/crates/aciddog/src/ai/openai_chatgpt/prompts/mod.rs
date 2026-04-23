//! Bundled Codex CLI prompt files. The ChatGPT-subscription
//! `/responses` endpoint expects `instructions` to be set verbatim to
//! one of these files; user-supplied system prompts go into `input[]`
//! as a `role: "developer"` message instead.
//!
//! Files are frozen copies of `openai/codex`:
//!   - `default.md`       ← `codex-rs/protocol/src/prompts/base_instructions/default.md`
//!   - `gpt_5_codex.md`   ← `codex-rs/core/gpt_5_codex_prompt.md`
//!   - `gpt_5_1.md`       ← `codex-rs/core/gpt_5_1_prompt.md`
//!
//! When Codex ships a new model family, refresh these files and
//! update `instructions_for()` in a follow-up PR.

const DEFAULT: &str = include_str!("default.md");
const GPT_5_CODEX: &str = include_str!("gpt_5_codex.md");
const GPT_5_1: &str = include_str!("gpt_5_1.md");

/// Map a model slug to the prompt Codex CLI ships for that family.
/// Unknown slugs fall back to `default.md` — the backend may still
/// accept the request but responses will be generic.
pub fn instructions_for(model: &str) -> &'static str {
    let m = model.to_ascii_lowercase();
    // Codex-variant models (gpt-5-codex, gpt-5.1-codex, gpt-5.1-codex-max,
    // gpt-5.2-codex, …) all use the Codex prompt.
    if m.contains("codex") && m.starts_with("gpt-5") {
        return GPT_5_CODEX;
    }
    // Non-codex GPT-5.1 / GPT-5.2 use the GPT-5.1 prompt template.
    if m.starts_with("gpt-5.1") || m.starts_with("gpt-5.2") {
        return GPT_5_1;
    }
    DEFAULT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_prompts_are_non_empty() {
        assert!(!DEFAULT.trim().is_empty());
        assert!(!GPT_5_CODEX.trim().is_empty());
        assert!(!GPT_5_1.trim().is_empty());
    }

    #[test]
    fn codex_models_select_codex_prompt() {
        assert!(instructions_for("gpt-5-codex").starts_with("You are Codex"));
        assert!(instructions_for("gpt-5.1-codex").starts_with("You are Codex"));
        assert!(instructions_for("gpt-5.1-codex-max").starts_with("You are Codex"));
        assert!(instructions_for("gpt-5.2-codex").starts_with("You are Codex"));
    }

    #[test]
    fn non_codex_gpt5_selects_gpt_5_1_prompt() {
        let p = instructions_for("gpt-5.1");
        assert!(p.starts_with("You are GPT-5.1"));
        let p = instructions_for("gpt-5.2");
        assert!(p.starts_with("You are GPT-5.1"));
    }

    #[test]
    fn unknown_model_falls_back_to_default() {
        let p = instructions_for("some-future-model");
        assert!(p.starts_with("You are a coding agent"));
    }
}
