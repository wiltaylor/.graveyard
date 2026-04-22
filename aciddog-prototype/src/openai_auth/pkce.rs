//! PKCE (RFC 7636) S256 verifier + challenge generation. The OpenAI
//! `/oauth/authorize` flow used by Codex CLI and this core requires
//! `code_challenge_method=S256`. Verifier format: 43-128 character
//! url-safe base64 over 32 random bytes; challenge =
//! `base64url(sha256(verifier))`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        getrandom(&mut bytes);
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = challenge_for(&verifier);
        Self {
            verifier,
            challenge,
        }
    }
}

fn challenge_for(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// Generate a base64url `state` parameter (32 random bytes). Used for
/// CSRF protection on the OAuth callback.
pub fn random_state() -> String {
    let mut bytes = [0u8; 32];
    getrandom(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Minimal random bytes helper — uses `/dev/urandom` on unix,
/// `BCryptGenRandom` on windows via the `std`-level `Instant` seed
/// fallback isn't good enough for CSRF tokens, so we actually open
/// `/dev/urandom`. Windows path not wired yet; the OAuth flow is
/// unix-first for now.
fn getrandom(buf: &mut [u8]) {
    use std::io::Read;
    let mut f = std::fs::File::open("/dev/urandom").expect("open /dev/urandom");
    f.read_exact(buf).expect("read /dev/urandom");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_is_url_safe_and_right_length() {
        let p = Pkce::generate();
        // 32 bytes → base64url no padding → 43 chars.
        assert_eq!(p.verifier.len(), 43);
        assert!(p
            .verifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn challenge_matches_sha256_of_verifier() {
        // Canonical example from RFC 7636 Appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(challenge_for(verifier), expected);
    }

    #[test]
    fn state_is_url_safe_base64() {
        let s = random_state();
        assert_eq!(s.len(), 43);
        assert!(s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
