//! Cron endpoints — REST wrappers + SSE proxy over the aciddog
//! `Cron*` RPCs. Mirrors [`super::workflows`]: a JSON list endpoint,
//! CRUD mutations that round-trip a single RPC each, and an SSE
//! passthrough for live mutation events.

use std::convert::Infallible;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::client::Client;
use crate::error::{AppError, AppResult};
use crate::ipc::RequestCmd;
use crate::routes::instances::load_row_for_routes;
use crate::routes::tasks::MutationResponse;
use crate::state::AppState;
use crate::wire_cron::WireCronJob;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronListResponse {
    pub ok: bool,
    pub jobs: Vec<WireCronJob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GET /api/instances/:id/cron
pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<CronListResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::CronListJson {
            target: None,
            include_disabled: true,
            mine: false,
        })
        .await?;
    if !resp.ok {
        return Ok(Json(CronListResponse {
            ok: false,
            jobs: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(CronListResponse {
            ok: false,
            jobs: Vec::new(),
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 CronListJson support"
                    .into(),
            ),
        }));
    };
    let jobs: Vec<WireCronJob> = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding cron list: {e}")))?;
    Ok(Json(CronListResponse {
        ok: true,
        jobs,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateCronRequest {
    pub target: String,
    /// `"message"` or `"command"`.
    pub kind: String,
    pub payload: String,
    /// 5-field cron expression for recurring jobs; ISO wall-clock
    /// datetime (`YYYY-MM-DDTHH:MM[:SS]`) for one-off.
    pub schedule: String,
    #[serde(default)]
    pub tz: Option<String>,
    #[serde(default)]
    pub oneoff: bool,
    #[serde(default)]
    pub catchup_on_startup: bool,
    #[serde(default)]
    pub disabled: bool,
}

/// POST /api/instances/:id/cron
pub async fn create(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateCronRequest>,
) -> AppResult<Json<MutationResponse>> {
    if req.target.trim().is_empty() {
        return Err(AppError::Validation("target is empty".into()));
    }
    if req.payload.trim().is_empty() {
        return Err(AppError::Validation("payload is empty".into()));
    }
    if req.schedule.trim().is_empty() {
        return Err(AppError::Validation("schedule is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::CronAdd {
            target: req.target,
            kind: req.kind,
            payload: req.payload,
            schedule: req.schedule,
            tz: req.tz.filter(|s| !s.trim().is_empty()),
            oneoff: req.oneoff,
            catchup_on_startup: req.catchup_on_startup,
            disabled: req.disabled,
        })
        .await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        message: resp.message,
        error: resp.error,
    }))
}

#[derive(Debug, Deserialize)]
pub struct PatchCronRequest {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub tz: Option<String>,
    #[serde(default)]
    pub oneoff: Option<bool>,
    #[serde(default)]
    pub catchup_on_startup: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// PATCH /api/instances/:id/cron/:job_id
pub async fn patch(
    State(state): State<AppState>,
    Path((id, job_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<PatchCronRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::CronUpdate {
            id: job_id,
            target: req.target,
            kind: req.kind,
            payload: req.payload,
            schedule: req.schedule,
            tz: req.tz,
            oneoff: req.oneoff,
            catchup_on_startup: req.catchup_on_startup,
            enabled: req.enabled,
        })
        .await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        message: resp.message,
        error: resp.error,
    }))
}

/// DELETE /api/instances/:id/cron/:job_id
pub async fn delete(
    State(state): State<AppState>,
    Path((id, job_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::CronRemove { id: job_id })
        .await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        message: resp.message,
        error: resp.error,
    }))
}

/// GET /api/instances/:id/cron/events
///
/// Opens an SSE stream against the webservice's `/v1/cron/events`
/// endpoint and forwards the raw byte stream to the browser.
pub async fn events(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Response> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let (upstream_status, stream) = client.open_cron_events().await?;

    if upstream_status != 200 {
        let status = StatusCode::from_u16(upstream_status).unwrap_or(StatusCode::BAD_GATEWAY);
        let mut collected = Vec::new();
        let mut s = stream;
        while let Some(chunk) = s.next().await {
            if let Ok(bytes) = chunk {
                collected.extend_from_slice(&bytes);
                if collected.len() > 4096 {
                    break;
                }
            } else {
                break;
            }
        }
        let body = String::from_utf8_lossy(&collected).into_owned();
        return Ok((status, body).into_response());
    }

    // Errors on the upstream stream close the SSE connection cleanly;
    // `EventSource` auto-reconnects.
    let byte_stream = stream.map(|res| match res {
        Ok(b) => Ok::<_, Infallible>(b),
        Err(e) => {
            tracing::debug!(error = %e, "upstream cron event stream ended with error");
            Ok(bytes::Bytes::new())
        }
    });

    let mut resp = Response::new(Body::from_stream(byte_stream));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    resp.headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::{TimeZone, Utc};
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tokio::net::TcpListener;

    use crate::auth::AuthConfig;
    use crate::db;
    use crate::model::instance::{self, CreateInstance, Transport};
    use crate::wire_cron::{WireCronPayload, WireCronSchedule, WireCronTz};

    fn wcl_cron_id(block_id: &str) -> Uuid {
        const NAMESPACE_CRON_WCL: Uuid = Uuid::from_u128(0x61842da1_8d65_4f99_9d56_fa64d84833b5);
        Uuid::new_v5(&NAMESPACE_CRON_WCL, block_id.as_bytes())
    }

    fn fixture_job(id: Uuid) -> WireCronJob {
        WireCronJob {
            id,
            target_agent: "nightly-maintainer".into(),
            payload: WireCronPayload::SlashCommand {
                command: "/maintain --dry-run".into(),
            },
            schedule: WireCronSchedule::Recurring {
                expression: "15 3 * * *".into(),
            },
            timezone: WireCronTz::Utc,
            catchup_on_startup: false,
            enabled: true,
            created_by: "operator".into(),
            created_at: Utc.with_ymd_and_hms(2026, 5, 3, 1, 2, 3).unwrap(),
            last_run: Some(Utc.with_ymd_and_hms(2026, 5, 3, 3, 15, 0).unwrap()),
            next_run: Some(Utc.with_ymd_and_hms(2026, 5, 4, 3, 15, 0).unwrap()),
        }
    }

    async fn cron_rpc_stub(Json(req): Json<Value>) -> Json<Value> {
        assert_eq!(req["cmd"], "cron_list_json");
        assert_eq!(req["target"], Value::Null);
        assert_eq!(req["include_disabled"], true);
        assert_eq!(req["mine"], false);

        let wcl_job = fixture_job(wcl_cron_id("nightly_maintenance"));
        let imperative_job = fixture_job(Uuid::new_v4());
        Json(json!({
            "ok": true,
            "data": [wcl_job, imperative_job],
        }))
    }

    async fn spawn_stub_daemon() -> String {
        let app = axum::Router::new().route("/v1/rpc", axum::routing::post(cron_rpc_stub));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn list_returns_wcl_v5_and_imperative_v4_jobs_with_identical_schema() {
        let pool = db::connect_memory().await.unwrap();
        let base_url = spawn_stub_daemon().await;
        let instance = instance::insert(
            &pool,
            CreateInstance {
                name: "stub".into(),
                transport: Transport::Https,
                url: Some(base_url),
                socket_path: None,
                api_key: Some("test-token".into()),
                tls_ca_path: None,
            },
        )
        .await
        .unwrap();
        let aciddog_binary = Arc::new(std::path::PathBuf::from("/bin/true"));
        let runner = crate::daemon_runner::DaemonRunner::new(
            (*aciddog_binary).clone(),
            crate::daemon_runner::CapturePolicy::default(),
        );
        let state = AppState::new(
            pool,
            AuthConfig::default(),
            ".".into(),
            aciddog_binary,
            runner,
            crate::daemon_runner::CapturePolicy::default(),
        );

        let Json(resp) = list(State(state), Path(instance.id)).await.unwrap();

        assert!(resp.ok);
        assert!(resp.error.is_none());
        assert_eq!(resp.jobs.len(), 2);
        assert_eq!(resp.jobs[0].id, wcl_cron_id("nightly_maintenance"));
        assert_ne!(resp.jobs[1].id, resp.jobs[0].id);

        for job in &resp.jobs {
            assert_eq!(job.target_agent, "nightly-maintainer");
            assert_eq!(
                job.payload,
                WireCronPayload::SlashCommand {
                    command: "/maintain --dry-run".into()
                }
            );
            assert_eq!(
                job.schedule,
                WireCronSchedule::Recurring {
                    expression: "15 3 * * *".into()
                }
            );
            assert_eq!(job.timezone, WireCronTz::Utc);
            assert!(!job.catchup_on_startup);
            assert!(job.enabled);
            assert_eq!(job.created_by, "operator");
            assert!(job.last_run.is_some());
            assert!(job.next_run.is_some());
        }

        let round_tripped: CronListResponse =
            serde_json::from_value(serde_json::to_value(&resp).unwrap()).unwrap();
        assert_eq!(round_tripped.jobs, resp.jobs);

        let mut wcl_shape = serde_json::to_value(&resp.jobs[0]).unwrap();
        let mut v4_shape = serde_json::to_value(&resp.jobs[1]).unwrap();
        wcl_shape["id"] = json!("<uuid>");
        v4_shape["id"] = json!("<uuid>");
        assert_eq!(wcl_shape, v4_shape);
    }
}
