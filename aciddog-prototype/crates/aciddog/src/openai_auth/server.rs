//! One-shot HTTP listener on `127.0.0.1:1455` for the OAuth callback.
//! Accepts exactly one request on `/auth/callback`, extracts `code`
//! and `state`, serves a small "you may close this tab" page, and
//! returns the captured values to the caller.

use std::collections::HashMap;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

use super::AuthError;

pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

/// Bind the callback port and wait up to `wait` for a single request.
/// Fails fast if the port is already in use (the user may have another
/// `aciddog` or `codex` instance running).
pub async fn wait_for_callback(wait: Duration) -> Result<CallbackResult, AuthError> {
    let listener = TcpListener::bind(("127.0.0.1", super::CALLBACK_PORT))
        .await
        .map_err(|e| {
            AuthError::CallbackBind(format!(
                "failed to bind 127.0.0.1:{} ({e}). Is another aciddog/codex login in progress?",
                super::CALLBACK_PORT
            ))
        })?;

    let accepted = timeout(wait, listener.accept())
        .await
        .map_err(|_| AuthError::CallbackTimeout)?;
    let (mut stream, _) = accepted.map_err(AuthError::Io)?;

    // Read the request line and headers. We don't need the body; the
    // code is in the query string.
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 512];
    loop {
        let n = stream.read(&mut chunk).await.map_err(AuthError::Io)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 32_768 {
            return Err(AuthError::CallbackMalformed(
                "request header exceeded 32KiB".into(),
            ));
        }
    }

    let head = std::str::from_utf8(&buf)
        .map_err(|_| AuthError::CallbackMalformed("request header is not utf-8".into()))?;
    let request_line = head
        .lines()
        .next()
        .ok_or_else(|| AuthError::CallbackMalformed("request has no request-line".into()))?;
    // GET /auth/callback?code=...&state=... HTTP/1.1
    let mut parts = request_line.split_whitespace();
    let _method = parts.next();
    let path_and_query = parts
        .next()
        .ok_or_else(|| AuthError::CallbackMalformed("no path".into()))?;
    let (path, query) = path_and_query
        .split_once('?')
        .unwrap_or((path_and_query, ""));
    if path != "/auth/callback" {
        write_html(&mut stream, 404, "Not Found").await.ok();
        return Err(AuthError::CallbackMalformed(format!(
            "unexpected callback path `{path}`"
        )));
    }

    let params = parse_query(query);
    if let Some(err) = params.get("error") {
        let desc = params.get("error_description").cloned().unwrap_or_default();
        write_html(&mut stream, 400, "Sign-in failed. Return to the terminal.")
            .await
            .ok();
        return Err(AuthError::OAuth(format!("{err}: {desc}")));
    }
    let code = params
        .get("code")
        .cloned()
        .ok_or_else(|| AuthError::CallbackMalformed("missing `code`".into()))?;
    let state = params
        .get("state")
        .cloned()
        .ok_or_else(|| AuthError::CallbackMalformed("missing `state`".into()))?;

    write_html(&mut stream, 200, SUCCESS_HTML).await.ok();

    Ok(CallbackResult { code, state })
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pair in q.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let key = urlencoding::decode(k).unwrap_or_default().into_owned();
        let val = urlencoding::decode(v).unwrap_or_default().into_owned();
        out.insert(key, val);
    }
    out
}

async fn write_html(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    body: &str,
) -> std::io::Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         content-type: text/html; charset=utf-8\r\n\
         content-length: {len}\r\n\
         connection: close\r\n\
         \r\n{body}",
        len = body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await
}

const SUCCESS_HTML: &str = "<!doctype html>
<html><head><meta charset=\"utf-8\"><title>aciddog signed in</title></head>
<body style=\"font-family: system-ui, sans-serif; max-width: 480px; margin: 4rem auto; text-align: center;\">
<h1>Signed in</h1>
<p>You can close this tab and return to your terminal.</p>
</body></html>";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_query_splits_pairs() {
        let q = parse_query("code=abc&state=xyz");
        assert_eq!(q.get("code").map(String::as_str), Some("abc"));
        assert_eq!(q.get("state").map(String::as_str), Some("xyz"));
    }

    #[test]
    fn parse_query_decodes_percent_escapes() {
        let q = parse_query("code=a%2Bb&state=x%2Fy");
        assert_eq!(q.get("code").map(String::as_str), Some("a+b"));
        assert_eq!(q.get("state").map(String::as_str), Some("x/y"));
    }

    #[test]
    fn parse_query_handles_empty() {
        let q = parse_query("");
        assert!(q.is_empty());
    }
}
