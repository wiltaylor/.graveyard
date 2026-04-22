//! Minimal JWT payload decoder. Only pulls the
//! `https://api.openai.com/auth.chatgpt_account_id` claim out of the
//! access/id tokens returned by the OpenAI OAuth flow — we don't
//! verify signatures (the backend already enforces them; we just
//! need the claim locally to set the mandatory `chatgpt-account-id`
//! header on API requests).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::Value;

const ACCOUNT_ID_CLAIM: &str = "https://api.openai.com/auth.chatgpt_account_id";

#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("JWT does not have three dot-separated segments")]
    Shape,
    #[error("failed to base64-decode JWT payload: {0}")]
    Base64(#[source] base64::DecodeError),
    #[error("JWT payload is not valid UTF-8")]
    Utf8,
    #[error("JWT payload is not valid JSON: {0}")]
    Json(#[source] serde_json::Error),
    #[error("JWT payload is missing the `{ACCOUNT_ID_CLAIM}` claim")]
    MissingClaim,
}

/// Decode the payload segment of a JWT without verifying the
/// signature. Returns the parsed JSON claim bag.
pub fn decode_payload(token: &str) -> Result<Value, JwtError> {
    let mut parts = token.split('.');
    let _header = parts.next().ok_or(JwtError::Shape)?;
    let payload = parts.next().ok_or(JwtError::Shape)?;
    let _sig = parts.next().ok_or(JwtError::Shape)?;
    if parts.next().is_some() {
        return Err(JwtError::Shape);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(payload.as_bytes())
        .or_else(|_| {
            // Some JWTs use plain base64 with padding. Fall back.
            base64::engine::general_purpose::STANDARD.decode(payload.as_bytes())
        })
        .map_err(JwtError::Base64)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| JwtError::Utf8)?;
    serde_json::from_str::<Value>(text).map_err(JwtError::Json)
}

/// Pull `chatgpt_account_id` from an id_token's claims. The claim
/// lives at a fully-qualified URL key (OpenAI namespaces all custom
/// claims that way).
pub fn extract_account_id(id_token: &str) -> Result<String, JwtError> {
    let payload = decode_payload(id_token)?;
    // The claim can live directly at the URL key, or nested under
    // `https://api.openai.com/auth` → `chatgpt_account_id`. Try both.
    if let Some(v) = payload.get(ACCOUNT_ID_CLAIM).and_then(Value::as_str) {
        return Ok(v.to_string());
    }
    if let Some(v) = payload
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(Value::as_str)
    {
        return Ok(v.to_string());
    }
    Err(JwtError::MissingClaim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    fn make_jwt(payload: &Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_string(payload).unwrap().as_bytes());
        format!("{header}.{body}.sig")
    }

    #[test]
    fn extracts_account_id_from_flat_claim() {
        let payload = serde_json::json!({
            "sub": "user_123",
            "https://api.openai.com/auth.chatgpt_account_id": "acc_42",
        });
        let token = make_jwt(&payload);
        assert_eq!(extract_account_id(&token).unwrap(), "acc_42");
    }

    #[test]
    fn extracts_account_id_from_nested_claim() {
        let payload = serde_json::json!({
            "sub": "user_123",
            "https://api.openai.com/auth": { "chatgpt_account_id": "acc_99" },
        });
        let token = make_jwt(&payload);
        assert_eq!(extract_account_id(&token).unwrap(), "acc_99");
    }

    #[test]
    fn rejects_token_without_account_id() {
        let payload = serde_json::json!({ "sub": "user_123" });
        let token = make_jwt(&payload);
        assert!(matches!(
            extract_account_id(&token),
            Err(JwtError::MissingClaim)
        ));
    }

    #[test]
    fn rejects_malformed_token() {
        assert!(matches!(
            extract_account_id("not-a-jwt"),
            Err(JwtError::Shape)
        ));
    }
}
