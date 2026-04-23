//! OAuth 2.0 (Authorization Code + PKCE S256) login for the OpenAI
//! ChatGPT subscription endpoint used by the `OpenAiChatgpt` AI core.
//! Reuses the public `client_id`, port-1455 callback, and
//! `codex_cli_simplified_flow=true` handshake that Codex CLI and the
//! opencode plugin use against `auth.openai.com`.
//!
//! Credentials are stored named (e.g. `personal`, `work`) so a single
//! aciddog install can hold multiple ChatGPT accounts — see
//! `storage.rs` for the on-disk layout and resolution order.
//!
//! Public surface:
//!   - `login_chatgpt(name)` — opens a browser, waits for the
//!     callback on 127.0.0.1:1455, exchanges the code for tokens,
//!     extracts the `chatgpt_account_id` claim, and persists.
//!   - `refresh_chatgpt(name)` — POSTs the stored refresh token for a
//!     fresh access token, persists the rotated credential.
//!   - `get_chatgpt` / `remove` / `list` — read-side helpers for the
//!     TUI/CLI commands.

pub mod jwt;
pub mod pkce;
pub mod server;
pub mod storage;

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use serde::Deserialize;
use tracing::debug;

pub use storage::{
    get_chatgpt, list, load_file, remove, resolve_load_path, save_chatgpt, save_file, AuthFile,
    AuthTokens, Credential, CredentialSummary,
};

/// Public OpenAI client id used by Codex CLI and the opencode plugin.
/// The ChatGPT-subscription entitlement is gated on this id, so we
/// reuse it rather than registering our own.
pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const ISSUER: &str = "https://auth.openai.com";
pub const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
pub const CALLBACK_PORT: u16 = 1455;
pub const SCOPE: &str = "openid profile email offline_access";
pub const ORIGINATOR: &str = "codex_cli_rs";

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

/// Used by the AI core to proactively refresh before each turn. Any
/// token with less than this time left is refreshed synchronously.
pub const REFRESH_LEEWAY: ChronoDuration = ChronoDuration::seconds(60);

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("no user config directory found — set $HOME or $XDG_CONFIG_HOME")]
    NoConfigDir,

    #[error("credential `{0}` not found — run `aciddog auth login chatgpt --name {0}`")]
    NotFound(String),

    #[error("credential `{name}` has provider `{found}`, expected `{expected}`")]
    ProviderMismatch {
        name: String,
        found: &'static str,
        expected: &'static str,
    },

    #[error("invalid credential name `{0}` — must match [a-z0-9_-]+, length 1..=64")]
    BadName(String),

    #[error("failed to bind OAuth callback listener: {0}")]
    CallbackBind(String),

    #[error("OAuth callback timed out — no browser redirect within 5 minutes")]
    CallbackTimeout,

    #[error("OAuth callback malformed: {0}")]
    CallbackMalformed(String),

    #[error("OAuth state mismatch — possible cross-site request forgery")]
    StateMismatch,

    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("token exchange failed: HTTP {status}: {body}")]
    TokenExchange { status: u16, body: String },

    #[error("refresh token has been invalidated (`{reason}`) — run `aciddog auth login chatgpt --name {credential}`")]
    MustRelogin { credential: String, reason: String },

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[source] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[source] serde_json::Error),

    #[error("JWT error: {0}")]
    Jwt(#[from] jwt::JwtError),
}

/// Run the full OAuth flow for `name` and persist the tokens. The
/// user must open the printed authorize URL in a browser (we also try
/// to launch it automatically). On success returns the freshly-saved
/// tokens.
pub async fn login_chatgpt(name: &str) -> Result<AuthTokens, AuthError> {
    storage::validate_name(name)?;

    // Reject up-front if the name is taken by a different provider.
    match storage::get_chatgpt(name) {
        Ok(_) | Err(AuthError::NotFound(_)) => {}
        Err(AuthError::ProviderMismatch { .. }) => {
            return Err(AuthError::ProviderMismatch {
                name: name.to_string(),
                found: "other",
                expected: "openai_chatgpt",
            });
        }
        Err(e) => return Err(e),
    }

    let pkce = pkce::Pkce::generate();
    let state = pkce::random_state();

    let authorize_url = build_authorize_url(&pkce.challenge, &state);
    println!("Open this URL in your browser to sign in:");
    println!("  {authorize_url}");
    // Best-effort browser launch via xdg-open / open / start. Not a
    // hard dep: the user can paste the URL manually if it fails.
    let _ = try_open_browser(&authorize_url);

    let cb = server::wait_for_callback(CALLBACK_TIMEOUT).await?;
    if cb.state != state {
        return Err(AuthError::StateMismatch);
    }

    let tokens = exchange_code(&cb.code, &pkce.verifier).await?;
    let _path = storage::save_chatgpt(name, tokens.clone())?;
    Ok(tokens)
}

/// Force a refresh and persist. Callers should prefer
/// `refresh_if_needed` to avoid wasting a round trip when the access
/// token is still valid.
pub async fn refresh_chatgpt(name: &str) -> Result<AuthTokens, AuthError> {
    storage::validate_name(name)?;
    let tokens = storage::get_chatgpt(name)?;
    let refreshed = refresh_token_request(&tokens.refresh_token, name).await?;
    let merged = AuthTokens {
        access_token: refreshed.access_token.unwrap_or(tokens.access_token),
        refresh_token: refreshed.refresh_token.unwrap_or(tokens.refresh_token),
        id_token: refreshed
            .id_token
            .clone()
            .unwrap_or(tokens.id_token.clone()),
        account_id: refreshed
            .id_token
            .as_deref()
            .and_then(|idt| jwt::extract_account_id(idt).ok())
            .unwrap_or(tokens.account_id),
        expires_at: Utc::now()
            + ChronoDuration::seconds(refreshed.expires_in.unwrap_or(3600) as i64),
    };
    let _path = storage::save_chatgpt(name, merged.clone())?;
    Ok(merged)
}

/// Return fresh tokens for `name`, refreshing if expiry is within
/// `REFRESH_LEEWAY`. This is the hot path for each AI turn.
pub async fn refresh_if_needed(name: &str) -> Result<AuthTokens, AuthError> {
    let tokens = storage::get_chatgpt(name)?;
    if tokens.expires_at - Utc::now() > REFRESH_LEEWAY {
        return Ok(tokens);
    }
    refresh_chatgpt(name).await
}

fn build_authorize_url(challenge: &str, state: &str) -> String {
    // These non-standard params are load-bearing: without them the
    // OpenAI consent page serves a different onboarding flow that
    // doesn't attach the ChatGPT entitlement to the returned tokens.
    let params = [
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", REDIRECT_URI),
        ("scope", SCOPE),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("originator", ORIGINATOR),
    ];
    let query: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{ISSUER}/oauth/authorize?{query}")
}

fn try_open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "windows")]
    let cmd = "start";

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        std::process::Command::new(cmd)
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<u64>,
}

async fn exchange_code(code: &str, verifier: &str) -> Result<AuthTokens, AuthError> {
    let client = reqwest::Client::new();
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", REDIRECT_URI),
        ("client_id", CLIENT_ID),
        ("code_verifier", verifier),
    ];
    let resp = client
        .post(format!("{ISSUER}/oauth/token"))
        .form(&form)
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(AuthError::TokenExchange {
            status: status.as_u16(),
            body,
        });
    }
    let parsed: TokenResponse = serde_json::from_str(&body).map_err(AuthError::Json)?;
    let access_token = parsed
        .access_token
        .ok_or_else(|| AuthError::TokenExchange {
            status: 200,
            body: "token response missing access_token".into(),
        })?;
    let refresh_token = parsed
        .refresh_token
        .ok_or_else(|| AuthError::TokenExchange {
            status: 200,
            body: "token response missing refresh_token".into(),
        })?;
    let id_token = parsed.id_token.ok_or_else(|| AuthError::TokenExchange {
        status: 200,
        body: "token response missing id_token".into(),
    })?;
    let account_id = jwt::extract_account_id(&id_token)?;
    let expires_at = Utc::now() + ChronoDuration::seconds(parsed.expires_in.unwrap_or(3600) as i64);
    debug!(
        account_id = %account_id,
        expires_at = %expires_at,
        "openai_auth: token exchange succeeded"
    );
    Ok(AuthTokens {
        access_token,
        refresh_token,
        id_token,
        account_id,
        expires_at,
    })
}

async fn refresh_token_request(
    refresh: &str,
    credential_name: &str,
) -> Result<TokenResponse, AuthError> {
    let client = reqwest::Client::new();
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
        ("client_id", CLIENT_ID),
    ];
    let resp = client
        .post(format!("{ISSUER}/oauth/token"))
        .form(&form)
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await?;
    if status.as_u16() == 401 {
        let reason = parse_refresh_error(&body);
        if matches!(
            reason.as_str(),
            "refresh_token_expired" | "refresh_token_reused" | "refresh_token_invalidated"
        ) {
            return Err(AuthError::MustRelogin {
                credential: credential_name.to_string(),
                reason,
            });
        }
    }
    if !status.is_success() {
        return Err(AuthError::TokenExchange {
            status: status.as_u16(),
            body,
        });
    }
    serde_json::from_str::<TokenResponse>(&body).map_err(AuthError::Json)
}

fn parse_refresh_error(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_contains_required_params() {
        let url = build_authorize_url("chal", "state");
        assert!(url.starts_with("https://auth.openai.com/oauth/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
        assert!(url.contains("code_challenge=chal"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=state"));
        assert!(url.contains("id_token_add_organizations=true"));
        assert!(url.contains("codex_cli_simplified_flow=true"));
        assert!(url.contains("originator=codex_cli_rs"));
    }

    #[test]
    fn parse_refresh_error_extracts_error_field() {
        let body = r#"{"error":"refresh_token_expired","error_description":"x"}"#;
        assert_eq!(parse_refresh_error(body), "refresh_token_expired");
    }

    #[test]
    fn parse_refresh_error_falls_back_to_unknown() {
        assert_eq!(parse_refresh_error("not-json"), "unknown");
    }
}
