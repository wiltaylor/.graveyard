//! Rate-limit/usage lookups for configured AI cores.
//!
//! Each supported [`UsageProvider`] knows where to fetch a utilization
//! percentage for the core it is attached to:
//!   * z.ai cores use the core's own `api_key_env`.
//!   * ChatGPT OAuth cores use our own stored OAuth credential (see
//!     `crate::openai_auth`) and hit the same `/wham/usage` endpoint
//!     the upstream Codex CLI uses for its `/status` command.
//!   * OpenRouter and GitHub Copilot have their own endpoints.
//!
//! `claudeCode` cores have no HTTP fetcher here — their reset-window
//! data is populated straight from `rate_limit_event` lines in the
//! claude CLI's stream-json output (see `crate::ai::event::RateLimitKind`
//! and `CoresScreen::merge_rate_limit`). This keeps us from touching
//! the user's Claude OAuth token at `~/.claude/.credentials.json`.
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

use crate::ai::event::RateLimitKind;

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
    /// Free-form line to render in place of the percentage cell — used
    /// by providers whose natural unit isn't a percentage (OpenRouter
    /// returns a dollar balance; GitHub Copilot returns a premium-request
    /// count). When `Some`, the Cores view renders this verbatim instead
    /// of constructing "5h:X% · wk:Y%".
    pub display_override: Option<String>,
    /// claudeCode-only: unix-second timestamp when the five-hour window
    /// rolls over. Populated from `rate_limit_event` stream lines, not
    /// HTTP. `None` for every other provider.
    pub claude_five_hour_resets_at: Option<i64>,
    /// claudeCode-only: unix-second timestamp when the weekly window
    /// rolls over. Populated from `rate_limit_event` stream lines.
    pub claude_weekly_resets_at: Option<i64>,
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
    // Gemini CLI: deliberately no provider. The public Gemini API has no
    // per-user quota endpoint; the only surface that reports remaining
    // quota is `cloudcode-pa.googleapis.com/v1internal`, which is
    // undocumented and changes without notice. Revisit if/when Google
    // ships a stable endpoint.
    let snap = match provider {
        UsageProvider::Zai => fetch_zai(core, http).await,
        UsageProvider::ChatgptOauth => fetch_chatgpt_oauth(core, http).await,
        UsageProvider::Openrouter => fetch_openrouter(core, http).await,
        UsageProvider::GithubCopilot => fetch_github_copilot(core, http).await,
    };
    Some(snap)
}

/// Request timeout. Kept short — the Cores view blocks on these when
/// the user opens it and we would rather show "timeout" than hang.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

// ── claudeCode stream-derived reset info ───────────────────────────

/// Render the Cores-view cell for a `claudeCode` core from the two
/// unix-second reset timestamps harvested from `rate_limit_event`
/// stream lines. Missing windows are skipped; with nothing known at
/// all we surface an "awaiting stream" placeholder so the row doesn't
/// look broken.
pub fn format_claude_stream_cell(five_hour: Option<i64>, weekly: Option<i64>) -> String {
    let fh = five_hour.and_then(format_reset_in_unix);
    let wk = weekly.and_then(format_reset_in_unix);
    match (fh, wk) {
        (Some(a), Some(b)) => format!("5h in {} · wk in {}", a, b),
        (Some(a), None) => format!("5h in {}", a),
        (None, Some(b)) => format!("wk in {}", b),
        (None, None) => "(awaiting stream)".to_string(),
    }
}

/// Fold a `rate_limit_event` into an existing snapshot, returning the
/// merged value. The two windows arrive in separate events (different
/// turns), so each only overwrites its own slot — merging ensures the
/// later event doesn't wipe the earlier one.
pub fn merge_rate_limit_into(
    prev: Option<UsageSnapshot>,
    kind: RateLimitKind,
    resets_at_unix: Option<i64>,
) -> UsageSnapshot {
    let mut snap = prev.unwrap_or_default();
    match kind {
        RateLimitKind::FiveHour => snap.claude_five_hour_resets_at = resets_at_unix,
        RateLimitKind::Weekly => snap.claude_weekly_resets_at = resets_at_unix,
    }
    snap.display_override = Some(format_claude_stream_cell(
        snap.claude_five_hour_resets_at,
        snap.claude_weekly_resets_at,
    ));
    snap.error = None;
    snap
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
        display_override: None,
        claude_five_hour_resets_at: None,
        claude_weekly_resets_at: None,
    }
}

// ── OpenRouter credits ─────────────────────────────────────────────

/// Base URL for the OpenRouter v1 API. Factored out so tests can point
/// `fetch_openrouter_at` at a wiremock host.
pub(crate) const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

#[derive(Deserialize)]
struct OpenrouterResponse {
    data: OpenrouterCredits,
}

#[derive(Deserialize)]
struct OpenrouterCredits {
    total_credits: f64,
    total_usage: f64,
}

async fn fetch_openrouter(core: &AiCore, http: reqwest::Client) -> UsageSnapshot {
    fetch_openrouter_at(core, http, OPENROUTER_BASE_URL).await
}

pub(crate) async fn fetch_openrouter_at(
    core: &AiCore,
    http: reqwest::Client,
    base_url: &str,
) -> UsageSnapshot {
    let env_var = match &core.kind {
        AiCoreKind::OpenAiApi { api_key_env, .. } => api_key_env.as_str(),
        _ => {
            return UsageSnapshot::error("usage_provider :openrouter requires an openAiApi core");
        }
    };
    let key = match std::env::var(env_var) {
        Ok(k) if !k.is_empty() => k,
        _ => return UsageSnapshot::error(format!("${} unset", env_var)),
    };

    let url = format!("{}/credits", base_url);
    let resp = http
        .get(&url)
        .bearer_auth(&key)
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
            return UsageSnapshot::error(format!("api key rejected — check ${}", env_var));
        }
        return UsageSnapshot::error(format!("http {}", status));
    }
    let payload: OpenrouterResponse = match resp.json().await {
        Ok(p) => p,
        Err(e) => return UsageSnapshot::error(format!("decode: {}", e)),
    };
    let remaining = payload.data.total_credits - payload.data.total_usage;
    UsageSnapshot {
        display_override: Some(format!("${:.2} left", remaining)),
        ..UsageSnapshot::default()
    }
}

// ── GitHub Copilot premium requests ────────────────────────────────

/// Base URL for the GitHub REST API. Factored out for test injection.
pub(crate) const GITHUB_API_BASE_URL: &str = "https://api.github.com";

#[derive(Deserialize)]
struct GithubUserLogin {
    login: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GithubPremiumUsage {
    #[serde(default)]
    usage_items: Vec<GithubUsageItem>,
    /// Billing cycle the `usageItems` apply to. Shape: `{"year": 2026,
    /// "month": 4}`. Used to format a "resets in Nd" countdown —
    /// GitHub cycles reset on the 1st of the following month, UTC.
    #[serde(default)]
    time_period: Option<GithubTimePeriod>,
}

#[derive(Deserialize, Default)]
struct GithubTimePeriod {
    #[serde(default)]
    year: i32,
    #[serde(default)]
    month: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubUsageItem {
    #[serde(default)]
    sku: String,
    /// Percentage of the monthly premium-request allowance consumed
    /// by this line item. The response emits one item per model; the
    /// fetcher sums them. Matches what the Copilot billing dashboard
    /// displays.
    #[serde(default)]
    gross_quantity: f64,
    /// Dollar cost of requests beyond plan allowance. Non-zero only
    /// when the user has exceeded the included premium-request pool.
    #[serde(default)]
    net_amount: f64,
}

async fn fetch_github_copilot(core: &AiCore, http: reqwest::Client) -> UsageSnapshot {
    let token = match gh_auth_token().await {
        Ok(t) => t,
        Err(e) => return UsageSnapshot::error(e),
    };
    fetch_github_copilot_at(core, http, GITHUB_API_BASE_URL, &token).await
}

/// Acquire a GitHub OAuth token by invoking `gh auth token`. Kept as a
/// separate async function so the fetch path is testable without a
/// subprocess: tests call `fetch_github_copilot_at` directly with a
/// canned token.
async fn gh_auth_token() -> Result<String, String> {
    let output = tokio::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .await
        .map_err(|e| {
            format!(
                "gh not found ({}) — install github cli and `gh auth login`",
                e
            )
        })?;
    if !output.status.success() {
        return Err("gh auth token failed — run `gh auth login`".to_string());
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err("gh auth token returned empty — run `gh auth login`".to_string());
    }
    Ok(token)
}

pub(crate) async fn fetch_github_copilot_at(
    core: &AiCore,
    http: reqwest::Client,
    base_url: &str,
    token: &str,
) -> UsageSnapshot {
    if !matches!(core.kind, AiCoreKind::CopilotCli { .. }) {
        return UsageSnapshot::error("usage_provider :github_copilot requires a copilotCli core");
    }

    // Step 1: resolve the authenticated user's login. Avoids guessing
    // and also confirms the token is reachable before we hit the more
    // restrictive billing endpoint.
    let user_url = format!("{}/user", base_url);
    let user_resp = http
        .get(&user_url)
        .bearer_auth(token)
        .header("User-Agent", "aciddog")
        .header("Accept", "application/vnd.github+json")
        .timeout(HTTP_TIMEOUT)
        .send()
        .await;
    let user_resp = match user_resp {
        Ok(r) => r,
        Err(e) => return UsageSnapshot::error(format!("http: {}", e)),
    };
    if !user_resp.status().is_success() {
        let status = user_resp.status().as_u16();
        if status == 401 {
            return UsageSnapshot::error("gh token rejected — run `gh auth refresh`");
        }
        return UsageSnapshot::error(format!("http {} from /user", status));
    }
    let login = match user_resp.json::<GithubUserLogin>().await {
        Ok(u) => u.login,
        Err(e) => return UsageSnapshot::error(format!("decode /user: {}", e)),
    };

    // Step 2: fetch premium-request usage for that user.
    let usage_url = format!(
        "{}/users/{}/settings/billing/premium_request/usage",
        base_url, login
    );
    let resp = http
        .get(&usage_url)
        .bearer_auth(token)
        .header("User-Agent", "aciddog")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .timeout(HTTP_TIMEOUT)
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return UsageSnapshot::error(format!("http: {}", e)),
    };
    let status = resp.status();
    if !status.is_success() {
        let code = status.as_u16();
        // gh tokens without the `user` scope get a 404 on this
        // endpoint. GitHub returns the same 404 for genuinely missing
        // users, but the authenticated-user lookup above already
        // succeeded, so the cause is almost always scope.
        if code == 404 || code == 403 {
            return UsageSnapshot::error(
                "gh token missing user scope — run `gh auth refresh -h github.com -s user`",
            );
        }
        return UsageSnapshot::error(format!("http {}", code));
    }
    let payload: GithubPremiumUsage = match resp.json().await {
        Ok(p) => p,
        Err(e) => return UsageSnapshot::error(format!("decode: {}", e)),
    };

    // Enterprise-managed seats return an empty list — surface that
    // distinctly rather than showing "0 premium used".
    if payload.usage_items.is_empty() {
        return UsageSnapshot {
            display_override: Some("managed by org".into()),
            ..UsageSnapshot::default()
        };
    }

    let premium_items = payload
        .usage_items
        .iter()
        .filter(|i| i.sku == "Copilot Premium Request");
    let gross_pct: f64 = premium_items.clone().map(|i| i.gross_quantity).sum();
    let overage_dollars: f64 = premium_items.map(|i| i.net_amount).sum();
    let reset = payload
        .time_period
        .as_ref()
        .and_then(|tp| format_billing_cycle_reset(tp.year, tp.month));

    let mut line = match reset {
        Some(r) => format!("mo:{}% ({})", clamp_pct(gross_pct), r),
        None => format!("mo:{}%", clamp_pct(gross_pct)),
    };
    if overage_dollars > 0.0 {
        line.push_str(&format!(" · ${:.2} over", overage_dollars));
    }
    UsageSnapshot {
        display_override: Some(line),
        ..UsageSnapshot::default()
    }
}

// ── shared helpers ─────────────────────────────────────────────────

fn clamp_pct(v: f64) -> u8 {
    v.round().clamp(0.0, 100.0) as u8
}

fn clamp_pct_i32(v: i32) -> u8 {
    v.clamp(0, 100) as u8
}

/// Format a remaining duration as "Xh Ym" (or "Nd Xh" when >1 day)
/// given a Unix timestamp in seconds. Returns `None` for malformed
/// input or a moment already in the past.
fn format_reset_in_unix(secs: i64) -> Option<String> {
    let when = DateTime::<Utc>::from_timestamp(secs, 0)?;
    format_duration_until(when)
}

/// GitHub billing cycles close on the 1st of the following calendar
/// month, UTC. Given the current cycle's `(year, month)`, format the
/// remaining time until rollover. Returns `None` for malformed input
/// or a cycle that has already ended.
fn format_billing_cycle_reset(year: i32, month: u32) -> Option<String> {
    let (next_year, next_month) = if month >= 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let when = NaiveDate::from_ymd_opt(next_year, next_month, 1)?
        .and_hms_opt(0, 0, 0)?
        .and_utc();
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

    fn openrouter_core(api_key_env: &str) -> AiCore {
        AiCore {
            id: "openrouter".into(),
            kind: AiCoreKind::OpenAiApi {
                api_url: "https://openrouter.ai/api/v1".into(),
                api_key_env: api_key_env.into(),
                model: "openai/gpt-5.4-mini".into(),
                system_prompt: None,
                default_tools: None,
            },
            usage_provider: Some(UsageProvider::Openrouter),
        }
    }

    fn copilot_core() -> AiCore {
        AiCore {
            id: "copilot".into(),
            kind: AiCoreKind::CopilotCli {
                model: None,
                default_tools: None,
            },
            usage_provider: Some(UsageProvider::GithubCopilot),
        }
    }

    #[tokio::test]
    async fn openrouter_round_trip_populates_display_override() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/credits"))
            .and(header("Authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": { "total_credits": 100.0, "total_usage": 42.5 }
            })))
            .mount(&server)
            .await;

        // Unique env var name so parallel tests don't contend.
        let env_var = "ACIDDOG_TEST_OPENROUTER_KEY_OK";
        std::env::set_var(env_var, "test-key");
        let core = openrouter_core(env_var);
        let snap = fetch_openrouter_at(&core, reqwest::Client::new(), &server.uri()).await;
        std::env::remove_var(env_var);

        assert!(snap.error.is_none(), "unexpected error: {:?}", snap.error);
        assert_eq!(snap.display_override.as_deref(), Some("$57.50 left"));
        assert_eq!(snap.five_hour_pct, None);
        assert_eq!(snap.week_pct, None);
    }

    #[tokio::test]
    async fn openrouter_401_surfaces_api_key_hint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/credits"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let env_var = "ACIDDOG_TEST_OPENROUTER_KEY_401";
        std::env::set_var(env_var, "bad-key");
        let core = openrouter_core(env_var);
        let snap = fetch_openrouter_at(&core, reqwest::Client::new(), &server.uri()).await;
        std::env::remove_var(env_var);

        let err = snap.error.expect("should be an error");
        assert!(
            err.contains("api key rejected") && err.contains(env_var),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn github_copilot_round_trip_renders_monthly_percentage() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "login": "wiltaylor"
            })))
            .mount(&server)
            .await;
        // Shape mirrors the real response. grossQuantity is the
        // percent-of-monthly-allowance consumed by each model; the
        // fetcher sums across models and excludes non-Copilot SKUs.
        // A far-future timePeriod keeps `format_billing_cycle_reset`
        // deterministic for the substring assertion below.
        Mock::given(method("GET"))
            .and(path(
                "/users/wiltaylor/settings/billing/premium_request/usage",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "timePeriod": { "year": 2999, "month": 4 },
                "usageItems": [
                    { "sku": "Copilot Premium Request", "grossQuantity": 15.51, "netAmount": 0.0 },
                    { "sku": "Copilot Premium Request", "grossQuantity": 11.0,  "netAmount": 0.0 },
                    { "sku": "Storage",                 "grossQuantity": 99.0,  "netAmount": 99.0 }
                ]
            })))
            .mount(&server)
            .await;

        let core = copilot_core();
        let snap =
            fetch_github_copilot_at(&core, reqwest::Client::new(), &server.uri(), "test-token")
                .await;

        assert!(snap.error.is_none(), "unexpected error: {:?}", snap.error);
        let line = snap.display_override.expect("display present");
        // 15.51 + 11.0 = 26.51 → clamp_pct rounds to 27.
        assert!(
            line.starts_with("mo:27% ("),
            "unexpected display line: {line}"
        );
        assert!(!line.contains("over"), "unexpected overage: {line}");
    }

    #[tokio::test]
    async fn github_copilot_surfaces_overage_charge_when_net_amount_nonzero() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "login": "wiltaylor"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/users/wiltaylor/settings/billing/premium_request/usage",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "timePeriod": { "year": 2999, "month": 4 },
                "usageItems": [
                    { "sku": "Copilot Premium Request", "grossQuantity": 110.0, "netAmount": 4.40 }
                ]
            })))
            .mount(&server)
            .await;

        let core = copilot_core();
        let snap =
            fetch_github_copilot_at(&core, reqwest::Client::new(), &server.uri(), "test-token")
                .await;

        let line = snap.display_override.expect("display present");
        assert!(line.starts_with("mo:100% ("), "unexpected line: {line}");
        assert!(
            line.ends_with("· $4.40 over"),
            "missing overage charge: {line}"
        );
    }

    #[tokio::test]
    async fn github_copilot_empty_items_shows_managed_by_org() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "login": "wiltaylor"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/users/wiltaylor/settings/billing/premium_request/usage",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "usageItems": [] })))
            .mount(&server)
            .await;

        let core = copilot_core();
        let snap =
            fetch_github_copilot_at(&core, reqwest::Client::new(), &server.uri(), "test-token")
                .await;

        assert!(snap.error.is_none(), "unexpected error: {:?}", snap.error);
        assert_eq!(snap.display_override.as_deref(), Some("managed by org"));
    }

    #[tokio::test]
    async fn github_copilot_scope_hint_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "login": "wiltaylor"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/users/wiltaylor/settings/billing/premium_request/usage",
            ))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "message": "Not Found"
            })))
            .mount(&server)
            .await;

        let core = copilot_core();
        let snap =
            fetch_github_copilot_at(&core, reqwest::Client::new(), &server.uri(), "test-token")
                .await;

        let err = snap.error.expect("should be an error");
        assert!(
            err.contains("missing user scope") && err.contains("gh auth refresh"),
            "unexpected error: {err}"
        );
    }
}
