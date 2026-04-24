//! Memory bank endpoints — bank list, entry search + CRUD, link
//! CRUD, and the full-bank graph snapshot.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::client::Client;
use crate::error::{AppError, AppResult};
use crate::ipc::RequestCmd;
use crate::routes::instances::load_row_for_routes;
use crate::state::AppState;
use crate::wire_memory::{WireBankInfo, WireGraph, WireHit, WireLink};

#[derive(Debug, Clone, Serialize)]
pub struct BanksResponse {
    pub ok: bool,
    pub banks: Vec<WireBankInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub ok: bool,
    pub hits: Vec<WireHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinksResponse {
    pub ok: bool,
    pub links: Vec<WireLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<WireGraph>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MutationResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
}

impl MutationResponse {
    fn from_ipc(resp: crate::ipc::IpcResponse) -> Self {
        let id = resp
            .data
            .as_ref()
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        Self {
            ok: resp.ok,
            message: resp.message,
            error: resp.error,
            id,
        }
    }
}

/// GET /api/instances/:id/memory/banks
pub async fn list_banks(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<BanksResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::MemoryBanks).await?;
    if !resp.ok {
        return Ok(Json(BanksResponse {
            ok: false,
            banks: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(BanksResponse {
            ok: false,
            banks: Vec::new(),
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 MemoryBanks support"
                    .into(),
            ),
        }));
    };
    let banks: Vec<WireBankInfo> = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding memory banks: {e}")))?;
    Ok(Json(BanksResponse {
        ok: true,
        banks,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub title_contains: Option<String>,
    #[serde(default)]
    pub include_tags: Option<String>,
    #[serde(default)]
    pub exclude_tags: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

fn split_tags(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// GET /api/instances/:id/memory/banks/:bank/search
pub async fn search(
    State(state): State<AppState>,
    Path((id, bank)): Path<(Uuid, String)>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<SearchResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::MemorySearchJson {
            bank,
            query: q.q.filter(|s| !s.trim().is_empty()),
            title_contains: q.title_contains.filter(|s| !s.trim().is_empty()),
            include_tags: split_tags(&q.include_tags),
            exclude_tags: split_tags(&q.exclude_tags),
            limit: q.limit,
        })
        .await?;
    if !resp.ok {
        return Ok(Json(SearchResponse {
            ok: false,
            hits: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let hits: Vec<WireHit> = resp
        .data
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding hits: {e}")))?
        .unwrap_or_default();
    Ok(Json(SearchResponse {
        ok: true,
        hits,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct StoreRequest {
    pub body: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// POST /api/instances/:id/memory/banks/:bank/entries
pub async fn store(
    State(state): State<AppState>,
    Path((id, bank)): Path<(Uuid, String)>,
    Json(req): Json<StoreRequest>,
) -> AppResult<Json<MutationResponse>> {
    if req.body.trim().is_empty() {
        return Err(AppError::Validation("body is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::MemoryStoreJson {
            bank,
            body: req.body,
            title: req.title.filter(|s| !s.trim().is_empty()),
            tags: req.tags,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateRequest {
    pub body: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// PATCH /api/instances/:id/memory/banks/:bank/entries/:entry_id
pub async fn update(
    State(state): State<AppState>,
    Path((id, bank, entry_id)): Path<(Uuid, String, Uuid)>,
    Json(req): Json<UpdateRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::MemoryUpdate {
            bank,
            id: entry_id,
            body: req.body,
            title: req.title,
            tags: req.tags,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

/// DELETE /api/instances/:id/memory/banks/:bank/entries/:entry_id
pub async fn remove(
    State(state): State<AppState>,
    Path((id, bank, entry_id)): Path<(Uuid, String, Uuid)>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::MemoryRemove { bank, id: entry_id })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

#[derive(Debug, Deserialize)]
pub struct LinksQuery {
    #[serde(default)]
    pub direction: Option<String>,
}

/// GET /api/instances/:id/memory/banks/:bank/entries/:entry_id/links
pub async fn links(
    State(state): State<AppState>,
    Path((id, bank, entry_id)): Path<(Uuid, String, Uuid)>,
    Query(q): Query<LinksQuery>,
) -> AppResult<Json<LinksResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::MemoryLinksJson {
            bank,
            id: entry_id,
            direction: q.direction,
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
        })
        .await?;
    if !resp.ok {
        return Ok(Json(LinksResponse {
            ok: false,
            links: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let links: Vec<WireLink> = resp
        .data
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding links: {e}")))?
        .unwrap_or_default();
    Ok(Json(LinksResponse {
        ok: true,
        links,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct LinkRequest {
    pub from: Uuid,
    pub to: Uuid,
    #[serde(default)]
    pub kind: Option<String>,
}

/// POST /api/instances/:id/memory/banks/:bank/links
pub async fn add_link(
    State(state): State<AppState>,
    Path((id, bank)): Path<(Uuid, String)>,
    Json(req): Json<LinkRequest>,
) -> AppResult<Json<MutationResponse>> {
    if req.from == req.to {
        return Err(AppError::Validation(
            "from and to must differ".into(),
        ));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::MemoryLink {
            bank,
            from: req.from,
            to: req.to,
            kind: req.kind.filter(|s| !s.trim().is_empty()),
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

#[derive(Debug, Deserialize)]
pub struct UnlinkRequest {
    pub from: Uuid,
    pub to: Uuid,
}

/// DELETE /api/instances/:id/memory/banks/:bank/links
pub async fn remove_link(
    State(state): State<AppState>,
    Path((id, bank)): Path<(Uuid, String)>,
    Json(req): Json<UnlinkRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::MemoryUnlink {
            bank,
            from: req.from,
            to: req.to,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

/// GET /api/instances/:id/memory/banks/:bank/graph
pub async fn graph(
    State(state): State<AppState>,
    Path((id, bank)): Path<(Uuid, String)>,
) -> AppResult<Json<GraphResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::MemoryGraph { bank }).await?;
    if !resp.ok {
        return Ok(Json(GraphResponse {
            ok: false,
            graph: None,
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(GraphResponse {
            ok: false,
            graph: None,
            error: Some("remote returned ok but no `data`".into()),
        }));
    };
    let g: WireGraph = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding graph: {e}")))?;
    Ok(Json(GraphResponse {
        ok: true,
        graph: Some(g),
        error: None,
    }))
}
