//! Built-in tool that lets an agent perform skills-management actions
//! against the manager portal's HTTP API.
//!
//! Auth and base URL are read from environment variables — the same
//! pair the operator already configures on a manager-portal webservice
//! automaton:
//!   - `ACIDDOG_MANAGER_URL`: e.g. `https://manager.example.local:7000`
//!   - `ACIDDOG_MANAGER_TOKEN`: the per-instance api_key the manager
//!     stores in its `instances.api_key` column. Reused as a bearer
//!     token for the inbound `/api/skills/*` and `/api/marketplaces/*`
//!     surface (see `auth::require_user_or_instance`).
//!
//! The tool does not let the agent set an arbitrary URL — base URL is
//! fixed by the env var — so the surface stays auditable. Operators
//! decide which agents get the tool by adding `"ManagerSkillRequest"`
//! to the relevant `ai_core`'s `default_tools`.

use std::env;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolOutput};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct ManagerSkillRequestTool {
    client: reqwest::Client,
}

impl Default for ManagerSkillRequestTool {
    fn default() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("aciddog/0.1 (+manager-skill-request)")
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .expect("build reqwest client");
        Self { client }
    }
}

#[async_trait]
impl Tool for ManagerSkillRequestTool {
    fn name(&self) -> &'static str {
        "ManagerSkillRequest"
    }

    fn description(&self) -> &'static str {
        "Request skills-management actions on the aciddog manager portal. \
         Sub-actions: list_catalog, list_marketplaces, push_to_instance \
         (params: skill_id, instance_ids), import_from_instance \
         (params: instance_id, skill_name), update_marketplace \
         (params: marketplace_id), update_skill (params: skill_id), \
         list_instance_skills (params: instance_id). \
         Auth + base URL come from ACIDDOG_MANAGER_URL and \
         ACIDDOG_MANAGER_TOKEN env vars; the agent never sees them."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "list_catalog",
                        "list_marketplaces",
                        "list_instance_skills",
                        "push_to_instance",
                        "import_from_instance",
                        "update_marketplace",
                        "update_skill",
                    ],
                    "description": "Which manager-side action to perform."
                },
                "skill_id": {
                    "type": "string",
                    "description": "Catalog skill id (e.g. `uploaded--code-review`). Required for push_to_instance, update_skill."
                },
                "instance_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Manager-side instance ids. Required for push_to_instance."
                },
                "instance_id": {
                    "type": "string",
                    "description": "Single manager-side instance id. Required for import_from_instance, list_instance_skills."
                },
                "skill_name": {
                    "type": "string",
                    "description": "Skill directory name on a remote instance. Required for import_from_instance."
                },
                "marketplace_id": {
                    "type": "string",
                    "description": "Marketplace id. Required for update_marketplace."
                }
            },
            "required": ["action"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let action = match input.get("action").and_then(Value::as_str) {
            Some(a) => a.to_string(),
            None => return ToolOutput::error("ManagerSkillRequest: `action` is required"),
        };
        let base_url = match env::var("ACIDDOG_MANAGER_URL") {
            Ok(v) if !v.is_empty() => v.trim_end_matches('/').to_string(),
            _ => {
                return ToolOutput::error(
                    "ManagerSkillRequest: ACIDDOG_MANAGER_URL is unset; ask the operator to wire \
                     this agent's core to the manager portal",
                );
            }
        };
        let token = match env::var("ACIDDOG_MANAGER_TOKEN") {
            Ok(v) if !v.is_empty() => v,
            _ => {
                return ToolOutput::error(
                    "ManagerSkillRequest: ACIDDOG_MANAGER_TOKEN is unset; ask the operator to \
                     expose the instance api_key as this env var",
                );
            }
        };

        let (method, path, body) = match action.as_str() {
            "list_catalog" => ("GET", "/api/skills".to_string(), None),
            "list_marketplaces" => ("GET", "/api/marketplaces".to_string(), None),
            "list_instance_skills" => {
                let id = match input.get("instance_id").and_then(Value::as_str) {
                    Some(s) => s,
                    None => {
                        return ToolOutput::error("list_instance_skills requires `instance_id`");
                    }
                };
                ("GET", format!("/api/skills/instances/{id}"), None)
            }
            "push_to_instance" => {
                let skill_id = match input.get("skill_id").and_then(Value::as_str) {
                    Some(s) => s,
                    None => {
                        return ToolOutput::error("push_to_instance requires `skill_id`");
                    }
                };
                let ids = match input.get("instance_ids").and_then(Value::as_array) {
                    Some(arr) => arr
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect::<Vec<_>>(),
                    None => {
                        return ToolOutput::error(
                            "push_to_instance requires `instance_ids` (array)",
                        );
                    }
                };
                let body = json!({ "instance_ids": ids });
                ("POST", format!("/api/skills/{skill_id}/push"), Some(body))
            }
            "import_from_instance" => {
                let id = match input.get("instance_id").and_then(Value::as_str) {
                    Some(s) => s,
                    None => {
                        return ToolOutput::error("import_from_instance requires `instance_id`");
                    }
                };
                let name = match input.get("skill_name").and_then(Value::as_str) {
                    Some(s) => s,
                    None => {
                        return ToolOutput::error("import_from_instance requires `skill_name`");
                    }
                };
                let body = json!({ "instance_id": id, "skill_name": name });
                ("POST", "/api/skills/import".to_string(), Some(body))
            }
            "update_marketplace" => {
                let id = match input.get("marketplace_id").and_then(Value::as_str) {
                    Some(s) => s,
                    None => {
                        return ToolOutput::error("update_marketplace requires `marketplace_id`");
                    }
                };
                ("POST", format!("/api/marketplaces/{id}/update"), None)
            }
            "update_skill" => {
                let id = match input.get("skill_id").and_then(Value::as_str) {
                    Some(s) => s,
                    None => {
                        return ToolOutput::error("update_skill requires `skill_id`");
                    }
                };
                ("POST", format!("/api/skills/{id}/update"), None)
            }
            other => {
                return ToolOutput::error(format!("ManagerSkillRequest: unknown action `{other}`"));
            }
        };

        let url = format!("{base_url}{path}");
        let mut req = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            _ => unreachable!(),
        };
        req = req.bearer_auth(token);
        if let Some(body) = body {
            req = req.json(&body);
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                return ToolOutput::error(format!("ManagerSkillRequest: {url}: {e}"));
            }
        };
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let header = format!("HTTP {status} {method} {path}\n");
        let combined = format!("{header}{text}");
        if status.is_success() {
            ToolOutput::ok(combined)
        } else {
            ToolOutput::error(combined)
        }
    }
}
