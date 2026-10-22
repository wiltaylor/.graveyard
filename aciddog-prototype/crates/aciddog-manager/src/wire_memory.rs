//! Deserialize/serialize mirror of
//! `aciddog::wire_memory::{WireBankInfo, WireHit, WireLink, WireGraph…}`.
//!
//! Same duplication rationale as the other `wire_*.rs` modules —
//! canonical shape is in `crates/aciddog/src/wire_memory.rs`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireLinkDirection {
    Out,
    In,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireLink {
    pub peer_id: Uuid,
    pub peer_title: String,
    pub peer_body: String,
    #[serde(default)]
    pub peer_tags: Vec<String>,
    pub kind: String,
    pub direction: WireLinkDirection,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGraphNode {
    pub id: Uuid,
    pub title: String,
    #[serde(default)]
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
    #[serde(default)]
    pub nodes: Vec<WireGraphNode>,
    #[serde(default)]
    pub edges: Vec<WireGraphEdge>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_round_trips_no_distance() {
        let wire = r#"{
            "id":"00000000-0000-0000-0000-000000000001",
            "title":"t","body":"b","tags":["x"],
            "created_at":"2026-04-24T00:00:00Z"
        }"#;
        let h: WireHit = serde_json::from_str(wire).unwrap();
        assert_eq!(h.title, "t");
        assert!(h.distance.is_none());
        assert_eq!(h.tags, vec!["x"]);
    }

    #[test]
    fn graph_round_trips() {
        let wire = r#"{
            "bank":"notes",
            "nodes":[
                {"id":"00000000-0000-0000-0000-000000000001","title":"a","tags":[],"created_at":"2026-04-24T00:00:00Z"}
            ],
            "edges":[
                {"from":"00000000-0000-0000-0000-000000000001",
                 "to":"00000000-0000-0000-0000-000000000002",
                 "kind":"related","created_at":"2026-04-24T00:00:00Z"}
            ]
        }"#;
        let g: WireGraph = serde_json::from_str(wire).unwrap();
        assert_eq!(g.bank, "notes");
        assert_eq!(g.nodes.len(), 1);
        assert_eq!(g.edges.len(), 1);
    }
}
