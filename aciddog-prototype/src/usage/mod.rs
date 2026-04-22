//! Rate-limit/usage lookups for configured AI cores.
//!
//! Each supported [`UsageProvider`] knows where to fetch a utilization
//! percentage for the core it is attached to:
//!   * Anthropic OAuth cores piggy-back on the Claude Code credentials
//!     at `~/.claude/.credentials.json`.
//!   * z.ai cores use the core's own `api_key_env`.
//!   * ChatGPT OAuth cores use our own stored OAuth credential (see
//!     `crate::openai_auth`) and hit the same `/wham/usage` endpoint
//!     the upstream Codex CLI uses for its `/status` command.
//!
//! This module is intentionally HTTP-only; the rendering lives in the
//! TUI's Cores view.
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::config::{AiCore, AiCoreKind, UsageProvider};
use crate::openai_auth;

/// Utilization snapshot shown for a single core in the Cores view.
/// `error` is populated on failure and takes precedence over the
/// percentage columns.
#[derive(Debug, Clone, Default)]
pub struct UsageSnapshot {
    pub five_hour_pct: Option<u8>,
    /// Weekly-window percentage. Populated by Anthropic OAuth and
    /// ChatGPT OAuth; the z.ai endpoint only exposes a single window
    /// and leaves this `None`.
    pub week_pct: Option<u8>,
    /// Pre-formatted "Xh Ym" reset countdown, when known.
    pub reset_display: Option<String>,
    pub error: Option<String>,
}

impl UsageSnapshot {
    fn error(msg: impl Into<String>) -> Self {
        Self {
            error: Some(msg.into()),
            ..Self::default()
        }
    }
}

/// Fetch usage for a single core. Returns `None` when the core has no
/// resolvable [`UsageProvider`]; callers should display a dash in that
/// case. Any fetch failure produces `Some(UsageSnapshot { error, .. })`
/// rather than an `Err`, so rendering is uniform.
pub async fn fetch_usage(core: &AiCore, http: reqwest::Client) -> Option<UsageSnapshot> {
    let provider = core.effective_usage_provider()?;
    let snap = match provider {
        UsageProvider::AnthropicOauth => fetch_anthropic_oauth(http).await,
        UsageProvider::Zai => fetch_zai(core, http).await,
        UsageProvider::ChatgptOauth => fetch_chatgpt_oauth(core, http).await,
    };
    Some(snap)
}

/// Request timeout. Kept short — the Cores view blocks on these when
/// the user opens it and we would rather show "timeout" than hang.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

// ── Anthropic OAuth ────────────────────────────────────────────────

#[derive(Deserialize)]
struct AnthropicCreds {
    #[serde(rename = "claudeAiOauth")]
    oauth: AnthropicOauth,
}

#[derive(Deserialize)]
struct AnthropicOauth {
    #[serde(rename = "accessToken")]
    access_token: String,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    five_hour: AnthropicWindow,
    seven_day: AnthropicWindow,
}

#[derive(Deserialize)]
struct AnthropicWindow {
    utilization: f64,
    #[serde(default)]
    resets_at: Option<String>,
}

async fn fetch_anthropic_oauth(http: reqwest::Client) -> UsageSnapshot {
    let home = match dirs::home_dir() {
        Some(p) => p,
        None => return UsageSnapshot::error("no home dir"),
    };
    let path = home.join(".claude").join(".credentials.json");
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => return UsageSnapshot::error(format!("read creds: {}", e)),
    };
    let creds: AnthropicCreds = match serde_json::from_str(&raw) {
        Ok(c) => c,
        Err(e) => return UsageSnapshot::error(format!("parse creds: {}", e)),
    };

    let resp = http
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(&creds.oauth.access_token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", "aciddog")
        .timeout(HTTP_TIMEOUT)
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return UsageSnapshot::error(format!("http: {}", e)),
    };
    if !resp.status().is_success() {
        return UsageSnapshot::error(format!("http {}", resp.status().as_u16()));
    }
    let usage: AnthropicUsage = match resp.json().await {
        Ok(u) => u,
        Err(e) => return UsageSnapshot::error(format!("decode: {}", e)),
    };

    UsageSnapshot {
        five_hour_pct: Some(clamp_pct(usage.five_hour.utilization)),
        week_pct: Some(clamp_pct(usage.seven_day.utilization)),
        reset_display: usage
            .five_hour
            .resets_at
            .as_deref()
            .and_then(format_reset_in),
        error: None,
    }
}

// ── z.ai quota ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ZaiResponse {
    data: ZaiData,
}

#[derive(Deserialize)]
struct ZaiData {
    limits: Vec<ZaiLimit>,
}

#[derive(Deserialize)]
struct ZaiLimit {
    #[serde(rename = "type")]
    ty: String,
    percentage: f64,
}

async fn fetch_zai(core: &AiCore, http: reqwest::Client) -> UsageSnapshot {
    let env_var = match &core.kind {
        AiCoreKind::AnthropicApi { api_key_env, .. }
        | AiCoreKind::OpenAiApi { api_key_env, .. } => api_key_env.as_str(),
        _ => {
            return UsageSnapshot::error(
                "usage_provider :zai requires an anthropicApi or openAiApi core",
            );
        }
    };
    let key = match std::env::var(env_var) {
        Ok(k) if !k.is_empty() => k,
        _ => return UsageSnapshot::error(format!("${} unset", env_var)),
    };

    let resp = http
        .get("https://api.z.ai/api/monitor/usage/quota/limit")
        .header("Authorization", key)
        .header("Accept-Language", "en-US,en")
        .header("Content-Type", "application/json")
        .timeout(HTTP_TIMEOUT)
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return UsageSnapshot::error(format!("http: {}", e)),
    };
    if !resp.status().is_success() {
        return UsageSnapshot::error(format!("http {}", resp.status().as_u16()));
    }
    let payload: ZaiResponse = match resp.json().await {
        Ok(p) => p,
        Err(e) => return UsageSnapshot::error(format!("decode: {}", e)),
    };
    let tokens = payload
        .data
        .limits
        .iter()
        .find(|l| l.ty == "TOKENS_LIMIT")
        .map(|l| clamp_pct(l.percentage));
    match tokens {
        Some(pct) => UsageSnapshot {
            five_hour_pct: Some(pct),
            ..UsageSnapshot::default()
        },
        None => UsageSnapshot::error("no TOKENS_LIMIT in response"),
    }
}

// ── ChatGPT OAuth (Codex /wham/usage) ──────────────────────────────

/// Base URL for the endpoint — factored out so tests can swap in a
/// wiremock host. Production callers use [`fetch_chatgpt_oauth`] which
/// passes this constant through.
pub(crate) const CHATGPT_USAGE_BASE_URL: &str = "https://chatgpt.com/backend-api";

// Response shape matches the upstream OpenAPI model
// `RateLimitStatusPayload` in `codex-rs/codex-backend-openapi-models`:
// snake_case throughout, `rate_limit` (singular) at the top level, and
// each window split by time horizon (`primary_window` = 5h,
// `secondary_window` = weekly).
#[derive(Deserialize, Default)]
struct WhamUsage {
    #[serde(default)]
    rate_limit: Option<WhamLimits>,
}

#[derive(Deserialize, Default)]
struct WhamLimits {
    #[serde(default)]
    primary_window: Option<WhamWindow>,
    #[serde(default)]
    secondary_window: Option<WhamWindow>,
}

#[derive(Deserialize)]
struct WhamWindow {
    used_percent: i32,
    /// Unix timestamp (seconds) at which this window rolls over.
    #[serde(default)]
    reset_at: Option<i64>,
}

async fn fetch_chatgpt_oauth(core: &AiCore, http: reqwest::Client) -> UsageSnapshot {
    fetch_chatgpt_oauth_at(core, http, CHATGPT_USAGE_BASE_URL).await
}

/// Implementation split so tests can point `base_url` at a wiremock
/// server. `base_url` must **not** include a trailing slash; the path
/// `/wham/usage` is appended verbatim (matches the upstream codex-cli
/// `PathStyle::ChatGptApi` behaviour).
pub(crate) async fn fetch_chatgpt_oauth_at(
    core: &AiCore,
    http: reqwest::Client,
    base_url: &str,
) -> UsageSnapshot {
    let credential = match &core.kind {
        AiCoreKind::OpenAiChatgpt { credential, .. } => credential.as_str(),
        _ => {
            return UsageSnapshot::error(
                "usage_provider :chatgpt_oauth requires an openAiChatgpt core",
            );
        }
    };

    let tokens = match openai_auth::refresh_if_needed(credential).await {
        Ok(t) => t,
        Err(e) => return UsageSnapshot::error(format!("auth: {}", e)),
    };

    fetch_wham_usage(
        http,
        base_url,
        &tokens.access_token,
        &tokens.account_id,
        credential,
    )
    .await
}

/// Pure HTTP portion of the ChatGPT usage fetch. Takes pre-resolved
/// OAuth credentials so it can be tested against a wiremock server
/// without seeding `auth.json` on disk. `credential_hint` is the
/// credential name used to phrase the "login expired" error.
async fn fetch_wham_usage(
    http: reqwest::Client,
    base_url: &str,
    access_token: &str,
    account_id: &str,
    credential_hint: &str,
) -> UsageSnapshot {
    let url = format!("{}/wham/usage", base_url);
    let resp = http
        .get(&url)
        .bearer_auth(access_token)
        .header("ChatGPT-Account-Id", account_id)
        .header("User-Agent", "aciddog")
        .header("Accept", "application/json")
        .timeout(HTTP_TIMEOUT)
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return UsageSnapshot::error(format!("http: {}", e)),
    };
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        if status == 401 {
            return UsageSnapshot::error(format!(
                "login expired — run `aciddog auth login chatgpt --name {}`",
                credential_hint
            ));
        }
        return UsageSnapshot::error(format!("http {}", status));
    }
    let payload: WhamUsage = match resp.json().await {
        Ok(p) => p,
        Err(e) => return UsageSnapshot::error(format!("decode: {}", e)),
    };

    let limits = payload.rate_limit.unwrap_or_default();
    let primary = limits.primary_window;
    let secondary = limits.secondary_window;
    UsageSnapshot {
        five_hour_pct: primary.as_ref().map(|w| clamp_pct_i32(w.used_percent)),
        week_pct: secondary.as_ref().map(|w| clamp_pct_i32(w.used_percent)),
        reset_display: primary
            .as_ref()
            .and_then(|w| w.reset_at)
            .and_then(format_reset_in_unix),
        error: None,
    }
}

// ── shared helpers ─────────────────────────────────────────────────

fn clamp_pct(v: f64) -> u8 {
    v.round().clamp(0.0, 100.0) as u8
}

fn clamp_pct_i32(v: i32) -> u8 {
    v.clamp(0, 100) as u8
}

/// Given an RFC3339 instant in the future, format the remaining
/// duration as "Xh Ym" (dropping empty leading hours). Returns `None`
/// when the input is malformed or already in the past.
fn format_reset_in(s: &str) -> Option<String> {
    let when: DateTime<Utc> = s.parse().ok()?;
    format_duration_until(when)
}

/// Same as [`format_reset_in`] but takes a Unix timestamp in seconds
/// (the ChatGPT usage endpoint returns `resetsAt` as an int64).
fn format_reset_in_unix(secs: i64) -> Option<String> {
    let when = DateTime::<Utc>::from_timestamp(secs, 0)?;
    format_duration_until(when)
}

fn format_duration_until(when: DateTime<Utc>) -> Option<String> {
    let now = Utc::now();
    let total = when.signed_duration_since(now).num_seconds();
    if total <= 0 {
        return None;
    }
    let days = total / 86_400;
    let hours = (total % 86_400) / 3600;
    let mins = (total % 3600) / 60;
    if days > 0 {
        Some(format!("{}d {}h", days, hours))
    } else if hours > 0 {
        Some(format!("{}h {}m", hours, mins))
    } else {
        Some(format!("{}m", mins))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn wham_usage_round_trip_populates_snapshot() {
        let server = MockServer::start().await;
        let reset_at = Utc::now().timestamp() + 5_400; // 90 minutes out
                                                       // Matches the upstream OpenAPI RateLimitStatusPayload shape
                                                       // returned by `GET /backend-api/wham/usage`.
        let body = json!({
            "plan_type": "plus",
            "rate_limit": {
                "allowed": true,
                "limit_reached": false,
                "primary_window": {
                    "used_percent": 42,
                    "limit_window_seconds": 18_000,
                    "reset_after_seconds": 5_400,
                    "reset_at": reset_at,
                },
                "secondary_window": {
                    "used_percent": 15,
                    "limit_window_seconds": 604_800,
                    "reset_after_seconds": 432_000,
                    "reset_at": reset_at,
                }
            },
            "credits": null,
            "additional_rate_limits": null,
            "rate_limit_reached_type": null
        });
        Mock::given(method("GET"))
            .and(path("/wham/usage"))
            .and(header("Authorization", "Bearer test-token"))
            .and(header("ChatGPT-Account-Id", "acct-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let snap = fetch_wham_usage(
            reqwest::Client::new(),
            &server.uri(),
            "test-token",
            "acct-123",
            "personal",
        )
        .await;

        assert!(snap.error.is_none(), "unexpected error: {:?}", snap.error);
        assert_eq!(snap.five_hour_pct, Some(42));
        assert_eq!(snap.week_pct, Some(15));
        // Clock moves during the test; tolerate "1h 29m" ↔ "1h 30m".
        let reset = snap.reset_display.expect("reset_display present");
        assert!(
            reset == "1h 30m" || reset == "1h 29m",
            "unexpected reset_display: {reset}"
        );
    }

    #[tokio::test]
    async fn wham_usage_401_surfaces_login_expired_hint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wham/usage"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let snap = fetch_wham_usage(
            reqwest::Client::new(),
            &server.uri(),
            "test-token",
            "acct-123",
            "personal",
        )
        .await;

        let err = snap.error.expect("should be an error");
        assert!(
            err.contains("login expired") && err.contains("personal"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn wham_usage_absent_rate_limit_produces_empty_snapshot() {
        // Servers return `"rate_limit": null` when no rate limits are
        // tracked for the plan (e.g. API cores, Pro). Must not error
        // — just show dashes.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wham/usage"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "plan_type": "plus", "rate_limit": null })),
            )
            .mount(&server)
            .await;

        let snap = fetch_wham_usage(
            reqwest::Client::new(),
            &server.uri(),
            "test-token",
            "acct-123",
            "personal",
        )
        .await;
        assert!(snap.error.is_none(), "unexpected error: {:?}", snap.error);
        assert_eq!(snap.five_hour_pct, None);
        assert_eq!(snap.week_pct, None);
    }
}
