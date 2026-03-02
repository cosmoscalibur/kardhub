//! OAuth helpers for GitHub authentication.
//!
//! Provides the GitHub OAuth Device Flow (for desktop) and Authorization Code
//! + PKCE helpers (for browser extensions). Token storage and expiry tracking.

use std::fmt;

use serde::{Deserialize, Serialize};

/// OAuth configuration for a GitHub OAuth App.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// GitHub OAuth App client ID.
    pub client_id: String,
    /// Required scopes (e.g. "repo", "read:org").
    pub scopes: Vec<String>,
}

/// An access token obtained from GitHub OAuth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    /// The access token value.
    pub access_token: String,
    /// Token type (usually "bearer").
    pub token_type: String,
    /// Scopes granted.
    pub scope: String,
}

/// Errors that may occur during OAuth flows.
#[derive(Debug)]
pub enum AuthError {
    /// HTTP request failed.
    Http(String),
    /// Deserialization error.
    Deserialize(String),
    /// The user denied authorization.
    AccessDenied,
    /// The device code has expired (user did not authorize in time).
    ExpiredToken,
    /// Authorization is still pending (user has not entered the code yet).
    AuthorizationPending,
    /// Polling too fast (slow down).
    SlowDown,
    /// Generic error from GitHub.
    GitHub(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(msg) => write!(f, "HTTP error: {msg}"),
            Self::Deserialize(msg) => write!(f, "deserialization error: {msg}"),
            Self::AccessDenied => write!(f, "access denied by user"),
            Self::ExpiredToken => write!(f, "device code expired"),
            Self::AuthorizationPending => write!(f, "authorization pending"),
            Self::SlowDown => write!(f, "polling too fast, slow down"),
            Self::GitHub(msg) => write!(f, "GitHub error: {msg}"),
        }
    }
}

impl std::error::Error for AuthError {}

// ── Device Flow (desktop) ────────────────────────────────────────────

/// Response from `POST https://github.com/login/device/code`.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    /// The device verification code.
    pub device_code: String,
    /// The code the user must enter at the verification URL.
    pub user_code: String,
    /// URL where the user enters the code.
    pub verification_uri: String,
    /// Seconds until the device code expires.
    pub expires_in: u64,
    /// Minimum polling interval in seconds.
    pub interval: u64,
}

/// Response from polling `POST https://github.com/login/oauth/access_token`.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Deserialize)]
struct DevicePollResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    _interval: Option<u64>,
}

/// Device Flow client for desktop OAuth.
///
/// Usage:
/// 1. Call [`DeviceFlow::request_device_code`] to get a user code + verification URL.
/// 2. Display the code to the user and direct them to the verification URL.
/// 3. Poll [`DeviceFlow::poll_for_token`] at the specified interval until a token is returned.
#[cfg(not(target_arch = "wasm32"))]
pub struct DeviceFlow {
    http: reqwest::Client,
    config: OAuthConfig,
}

#[cfg(not(target_arch = "wasm32"))]
impl DeviceFlow {
    /// Create a new Device Flow client.
    pub fn new(config: OAuthConfig) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("kardhub/0.1")
            .build()
            .expect("failed to build reqwest client");
        Self { http, config }
    }

    /// Step 1: Request a device code from GitHub.
    ///
    /// Returns a [`DeviceCodeResponse`] containing the user code and
    /// verification URL to show the user.
    pub async fn request_device_code(&self) -> Result<DeviceCodeResponse, AuthError> {
        let resp = self
            .http
            .post("https://github.com/login/device/code")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("scope", &self.config.scopes.join(" ")),
            ])
            .send()
            .await
            .map_err(|e| AuthError::Http(e.to_string()))?;

        resp.json::<DeviceCodeResponse>()
            .await
            .map_err(|e| AuthError::Deserialize(e.to_string()))
    }

    /// Step 3: Poll GitHub for an access token.
    ///
    /// Returns `Ok(Token)` when the user has authorized, or an appropriate
    /// `AuthError` variant indicating the current state.
    pub async fn poll_for_token(&self, device_code: &str) -> Result<Token, AuthError> {
        let resp = self
            .http
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .map_err(|e| AuthError::Http(e.to_string()))?;

        let poll: DevicePollResponse = resp
            .json()
            .await
            .map_err(|e| AuthError::Deserialize(e.to_string()))?;

        if let Some(token) = poll.access_token {
            return Ok(Token {
                access_token: token,
                token_type: poll.token_type.unwrap_or_else(|| "bearer".into()),
                scope: poll.scope.unwrap_or_default(),
            });
        }

        match poll.error.as_deref() {
            Some("authorization_pending") => Err(AuthError::AuthorizationPending),
            Some("slow_down") => Err(AuthError::SlowDown),
            Some("expired_token") => Err(AuthError::ExpiredToken),
            Some("access_denied") => Err(AuthError::AccessDenied),
            Some(other) => Err(AuthError::GitHub(other.to_string())),
            None => Err(AuthError::GitHub("unexpected empty response".into())),
        }
    }
}

// ── Authorization Code + PKCE (browser extension) ────────────────────

/// PKCE (Proof Key for Code Exchange) verifier and challenge.
///
/// Used by browser extensions for the Authorization Code flow.
/// The `verifier` is a random string; the `challenge` is its SHA-256
/// hash, base64url-encoded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkceChallenge {
    /// Random verifier string (43–128 chars, URL-safe).
    pub verifier: String,
    /// SHA-256 hash of the verifier, base64url-encoded (no padding).
    pub challenge: String,
}

/// Build the GitHub OAuth authorization URL for the auth code flow.
///
/// The extension opens this URL in a browser tab or via `chrome.identity`.
/// After the user authorizes, GitHub redirects to `redirect_uri` with a `code`.
pub fn build_auth_url(
    config: &OAuthConfig,
    redirect_uri: &str,
    state: &str,
    pkce: Option<&PkceChallenge>,
) -> String {
    let mut url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope={}&state={}",
        config.client_id,
        redirect_uri,
        config.scopes.join(" "),
        state,
    );
    if let Some(p) = pkce {
        url.push_str(&format!(
            "&code_challenge={}&code_challenge_method=S256",
            p.challenge
        ));
    }
    url
}

/// Exchange an authorization code for an access token.
///
/// This should be called after the user redirects back with a `code` parameter.
#[cfg(not(target_arch = "wasm32"))]
pub async fn exchange_code(
    config: &OAuthConfig,
    code: &str,
    redirect_uri: &str,
    pkce_verifier: Option<&str>,
) -> Result<Token, AuthError> {
    let http = reqwest::Client::builder()
        .user_agent("kardhub/0.1")
        .build()
        .map_err(|e| AuthError::Http(e.to_string()))?;

    let mut params = vec![
        ("client_id", config.client_id.as_str()),
        ("code", code),
        ("redirect_uri", redirect_uri),
    ];
    if let Some(v) = pkce_verifier {
        params.push(("code_verifier", v));
    }

    let resp = http
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::Http(e.to_string()))?;

    #[derive(Deserialize)]
    struct TokenResp {
        access_token: Option<String>,
        token_type: Option<String>,
        scope: Option<String>,
        error: Option<String>,
        error_description: Option<String>,
    }

    let result: TokenResp = resp
        .json()
        .await
        .map_err(|e| AuthError::Deserialize(e.to_string()))?;

    if let Some(token) = result.access_token {
        Ok(Token {
            access_token: token,
            token_type: result.token_type.unwrap_or_else(|| "bearer".into()),
            scope: result.scope.unwrap_or_default(),
        })
    } else {
        Err(AuthError::GitHub(
            result
                .error_description
                .or(result.error)
                .unwrap_or_else(|| "unknown error".into()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_serialization_round_trip() {
        let token = Token {
            access_token: "gho_abc123".into(),
            token_type: "bearer".into(),
            scope: "repo read:org".into(),
        };
        let json = serde_json::to_string(&token).unwrap();
        let deserialized: Token = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.access_token, "gho_abc123");
        assert_eq!(deserialized.scope, "repo read:org");
    }

    #[test]
    fn oauth_config_serialization() {
        let config = OAuthConfig {
            client_id: "Iv1.abc123".into(),
            scopes: vec!["repo".into(), "read:org".into()],
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("Iv1.abc123"));
        assert!(json.contains("repo"));
    }

    #[test]
    fn build_auth_url_without_pkce() {
        let config = OAuthConfig {
            client_id: "test_id".into(),
            scopes: vec!["repo".into()],
        };
        let url = build_auth_url(
            &config,
            "https://example.com/callback",
            "random_state",
            None,
        );
        assert!(url.contains("client_id=test_id"));
        assert!(url.contains("state=random_state"));
        assert!(!url.contains("code_challenge"));
    }

    #[test]
    fn build_auth_url_with_pkce() {
        let config = OAuthConfig {
            client_id: "test_id".into(),
            scopes: vec!["repo".into()],
        };
        let pkce = PkceChallenge {
            verifier: "verifier123".into(),
            challenge: "challenge_hash".into(),
        };
        let url = build_auth_url(
            &config,
            "https://example.com/callback",
            "state1",
            Some(&pkce),
        );
        assert!(url.contains("code_challenge=challenge_hash"));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn auth_error_display() {
        assert_eq!(AuthError::AccessDenied.to_string(), "access denied by user");
        assert_eq!(AuthError::ExpiredToken.to_string(), "device code expired");
        assert!(AuthError::SlowDown.to_string().contains("slow down"));
    }
}
