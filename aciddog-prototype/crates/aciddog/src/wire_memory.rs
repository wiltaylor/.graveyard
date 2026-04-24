//! Serde-friendly projection of memory bank entries, links, and
//! graph snapshots. Returned by the supervisor `Memory*Json` RPCs to
//! the manager web portal's Memory tab.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::memory::{Hit, Link, LinkDirection};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireBankInfo {
    pub id: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_model: Option<String>,
    pub entry_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireHit {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    /// Distance from the search query embedding. `NaN` when no
    /// semantic query ran — we emit it as `null` on the wire so the
    /// frontend can distinguish "pure-filter result" from a distance
    /// of zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f32>,
}

impl From<&Hit> for WireHit {
    fn from(h: &Hit) -> Self {
        let distance = if h.distance.is_nan() {
            None
        } else {
            Some(h.distance)
        };
        Self {
            id: h.id,
            title: h.title.clone(),
            body: h.body.clone(),
            tags: h.tags.clone(),
            created_at: h.created_at,
            distance,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireLinkDirection {
    Out,
    In,
    Both,
}

impl From<LinkDirection> for WireLinkDirection {
    fn from(d: LinkDirection) -> Self {
        match d {
            LinkDirection::Out => Self::Out,
            LinkDirection::In => Self::In,
            LinkDirection::Both => Self::Both,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireLink {
    pub peer_id: Uuid,
    pub peer_title: String,
    pub peer_body: String,
    pub peer_tags: Vec<String>,
    pub kind: String,
    pub direction: WireLinkDirection,
    pub created_at: DateTime<Utc>,
}

impl From<&Link> for WireLink {
    fn from(l: &Link) -> Self {
        Self {
            peer_id: l.peer_id,
            peer_title: l.peer_title.clone(),
            peer_body: l.peer_body.clone(),
            peer_tags: l.peer_tags.clone(),
            kind: l.kind.clone(),
            direction: l.direction.into(),
            created_at: l.created_at,
        }
    }
}

/// Minimal node projection for the graph view — strips the body for
/// compactness; callers fetch full detail via `MemoryShow` /
/// `MemorySearchJson` when opening a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGraphNode {
    pub id: Uuid,
    pub title: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGraphEdge {
    pub from: Uuid,
    pub to: Uuid,
    pub kind: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGraph {
    pub bank: String,
    pub nodes: Vec<WireGraphNode>,
    pub edges: Vec<WireGraphEdge>,
}
