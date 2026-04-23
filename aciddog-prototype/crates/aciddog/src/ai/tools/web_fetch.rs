use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolOutput};

const MAX_BYTES: usize = 256 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct WebFetchTool {
    client: reqwest::Client,
}

impl Default for WebFetchTool {
    fn default() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("aciddog/0.1 (+https://aciddog.local)")
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .expect("build reqwest client");
        Self { client }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &'static str {
        "WebFetch"
    }

    fn description(&self) -> &'static str {
        super::built_in_tool_doc("WebFetch")
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Absolute URL (http or https) to fetch."
                }
            },
            "required": ["url"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let url = match input.get("url").and_then(Value::as_str) {
            Some(u) => u.to_string(),
            None => return ToolOutput::error("WebFetch: `url` is required"),
        };

        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return ToolOutput::error("WebFetch: only http(s) URLs are supported");
        }

        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(format!("WebFetch: {url}: {e}")),
        };
        let status = resp.status();
        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) => return ToolOutput::error(format!("WebFetch: {url}: body read failed: {e}")),
        };

        let mut truncated = body;
        let too_long = truncated.len() > MAX_BYTES;
        if too_long {
            truncated.truncate(MAX_BYTES);
            truncated.push_str("\n... (truncated)");
        }

        let header = format!("HTTP {status} {url}\n");
        let text = format!("{header}{truncated}");
        if status.is_success() {
            ToolOutput::ok(text)
        } else {
            ToolOutput::error(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetches_body_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/hello"))
            .respond_with(ResponseTemplate::new(200).set_body_string("world"))
            .mount(&server)
            .await;

        let tool = WebFetchTool::default();
        let out = tool
            .invoke(json!({"url": format!("{}/hello", server.uri())}))
            .await;
        assert!(!out.is_error, "{}", out.text);
        assert!(out.text.contains("world"));
        assert!(out.text.contains("HTTP 200"));
    }

    #[tokio::test]
    async fn non_2xx_is_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/oops"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let tool = WebFetchTool::default();
        let out = tool
            .invoke(json!({"url": format!("{}/oops", server.uri())}))
            .await;
        assert!(out.is_error);
        assert!(out.text.contains("HTTP 500"));
    }

    #[tokio::test]
    async fn rejects_non_http() {
        let tool = WebFetchTool::default();
        let out = tool.invoke(json!({"url": "ftp://foo"})).await;
        assert!(out.is_error);
    }
}
