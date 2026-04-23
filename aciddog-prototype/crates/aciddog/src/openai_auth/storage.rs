//! On-disk storage for OAuth credentials. Named credentials so a
//! single aciddog install can hold multiple ChatGPT accounts
//! (e.g. `personal`, `work`).
//!
//! Resolution order on load (whole-file, not per-credential):
//!   1. `./.aciddog/auth.json` in the current working dir (per-repo override)
//!   2. `~/.config/aciddog/auth.json` (user default)
//!
//! The override wins entirely when both exist — credentials aren't
//! merged across the two files. This matches how `.env` vs shell env
//! resolution is intentionally kept simple here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::AuthError;

/// Credentials live under arbitrary user-chosen names. Names must
/// match `[a-z0-9_-]+`, length 1..=64.
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AuthFile {
    #[serde(default)]
    pub credentials: BTreeMap<String, Credential>,
}

/// One provider's worth of credential material. Tagged by provider
/// so we can add more OAuth (or API-key-in-file) providers later
/// without schema churn.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum Credential {
    OpenaiChatgpt(AuthTokens),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub account_id: String,
    pub expires_at: DateTime<Utc>,
}

/// Redacted shape for `aciddog auth list`. Never exposes raw tokens.
#[derive(Debug, Serialize, Clone)]
pub struct CredentialSummary {
    pub name: String,
    pub provider: &'static str,
    pub account_id: String,
    pub expires_at: DateTime<Utc>,
}

impl Credential {
    pub fn provider_tag(&self) -> &'static str {
        match self {
            Credential::OpenaiChatgpt(_) => "openai_chatgpt",
        }
    }
    pub fn summary(&self, name: &str) -> CredentialSummary {
        match self {
            Credential::OpenaiChatgpt(t) => CredentialSummary {
                name: name.to_string(),
                provider: "openai_chatgpt",
                account_id: t.account_id.clone(),
                expires_at: t.expires_at,
            },
        }
    }
}

/// Validate a credential name. Returns `Err(BadName)` for names that
/// wouldn't round-trip safely through JSON / shell / WCL.
pub fn validate_name(name: &str) -> Result<(), AuthError> {
    if name.is_empty() || name.len() > 64 {
        return Err(AuthError::BadName(name.to_string()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(AuthError::BadName(name.to_string()));
    }
    Ok(())
}

fn override_path() -> PathBuf {
    PathBuf::from(".aciddog").join("auth.json")
}

fn user_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("aciddog").join("auth.json"))
}

/// Resolve the auth-file path that `load_file` would currently use,
/// along with whether it exists. Used by the CLI to print where
/// changes will be written.
pub fn resolve_load_path() -> (Option<PathBuf>, bool) {
    let o = override_path();
    if o.exists() {
        return (Some(o), true);
    }
    if let Some(u) = user_path() {
        let exists = u.exists();
        return (Some(u), exists);
    }
    (None, false)
}

/// Load the auth file from the resolved path. Missing file → empty
/// `AuthFile`. Returns the path it read from (or would write back to)
/// so callers can persist to the same location.
pub fn load_file() -> Result<(AuthFile, PathBuf), AuthError> {
    let path = match resolve_load_path() {
        (Some(p), _) => p,
        (None, _) => {
            return Err(AuthError::NoConfigDir);
        }
    };
    if !path.exists() {
        return Ok((AuthFile::default(), path));
    }
    let bytes = std::fs::read(&path).map_err(AuthError::Io)?;
    if bytes.is_empty() {
        return Ok((AuthFile::default(), path));
    }
    let file: AuthFile = serde_json::from_slice(&bytes).map_err(AuthError::Json)?;
    Ok((file, path))
}

/// Write the file back atomically: write to `<path>.tmp` then
/// `rename`. Mode 0600 on unix.
pub fn save_file(file: &AuthFile, path: &Path) -> Result<(), AuthError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(AuthError::Io)?;
    }
    let json = serde_json::to_vec_pretty(file).map_err(AuthError::Json)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).map_err(AuthError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(AuthError::Io)?;
    }
    std::fs::rename(&tmp, path).map_err(AuthError::Io)?;
    Ok(())
}

/// Pull a `Credential::OpenaiChatgpt` by name. Returns
/// `NotFound` / `ProviderMismatch` as appropriate so callers can
/// render sensible error messages.
pub fn get_chatgpt(name: &str) -> Result<AuthTokens, AuthError> {
    validate_name(name)?;
    let (file, _) = load_file()?;
    match file.credentials.get(name) {
        Some(Credential::OpenaiChatgpt(t)) => Ok(t.clone()),
        // Kept for future provider variants — compiles down to an
        // unreachable branch today but will engage once another
        // Credential variant is added.
        #[allow(unreachable_patterns)]
        Some(other) => Err(AuthError::ProviderMismatch {
            name: name.to_string(),
            found: other.provider_tag(),
            expected: "openai_chatgpt",
        }),
        None => Err(AuthError::NotFound(name.to_string())),
    }
}

/// Save a `Credential::OpenaiChatgpt` under `name`, overwriting any
/// previous entry. Returns the path it wrote to.
pub fn save_chatgpt(name: &str, tokens: AuthTokens) -> Result<PathBuf, AuthError> {
    validate_name(name)?;
    let (mut file, path) = load_file()?;
    file.credentials
        .insert(name.to_string(), Credential::OpenaiChatgpt(tokens));
    save_file(&file, &path)?;
    Ok(path)
}

/// Remove a named credential. Error if absent so the CLI can
/// complain cleanly.
pub fn remove(name: &str) -> Result<PathBuf, AuthError> {
    validate_name(name)?;
    let (mut file, path) = load_file()?;
    if file.credentials.remove(name).is_none() {
        return Err(AuthError::NotFound(name.to_string()));
    }
    save_file(&file, &path)?;
    Ok(path)
}

/// Summaries for `aciddog auth list`, sorted by name.
pub fn list() -> Result<Vec<CredentialSummary>, AuthError> {
    let (file, _) = load_file()?;
    let mut out: Vec<CredentialSummary> = file
        .credentials
        .iter()
        .map(|(name, cred)| cred.summary(name))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tokens() -> AuthTokens {
        AuthTokens {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            id_token: "it".into(),
            account_id: "acc_1".into(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
        }
    }

    #[test]
    fn validate_name_accepts_common_shapes() {
        validate_name("personal").unwrap();
        validate_name("work_2").unwrap();
        validate_name("a-b-c").unwrap();
    }

    #[test]
    fn validate_name_rejects_bad_chars() {
        assert!(matches!(validate_name(""), Err(AuthError::BadName(_))));
        assert!(matches!(
            validate_name("Personal"),
            Err(AuthError::BadName(_))
        ));
        assert!(matches!(
            validate_name("with space"),
            Err(AuthError::BadName(_))
        ));
        assert!(matches!(
            validate_name("with/slash"),
            Err(AuthError::BadName(_))
        ));
    }

    #[test]
    fn auth_file_roundtrips_multiple_credentials() {
        let mut file = AuthFile::default();
        file.credentials.insert(
            "personal".into(),
            Credential::OpenaiChatgpt(sample_tokens()),
        );
        file.credentials
            .insert("work".into(), Credential::OpenaiChatgpt(sample_tokens()));
        let json = serde_json::to_string_pretty(&file).unwrap();
        let back: AuthFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, file);
        assert!(json.contains("\"provider\": \"openai_chatgpt\""));
    }

    #[test]
    fn credential_summary_hides_tokens() {
        let tokens = AuthTokens {
            access_token: "SECRET_ACCESS".into(),
            refresh_token: "SECRET_REFRESH".into(),
            id_token: "SECRET_ID".into(),
            account_id: "acc_1".into(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
        };
        let cred = Credential::OpenaiChatgpt(tokens);
        let summary = cred.summary("personal");
        let json = serde_json::to_string(&summary).unwrap();
        assert!(!json.contains("SECRET_ACCESS"));
        assert!(!json.contains("SECRET_REFRESH"));
        assert!(!json.contains("SECRET_ID"));
        assert!(json.contains("acc_1"));
    }
}
