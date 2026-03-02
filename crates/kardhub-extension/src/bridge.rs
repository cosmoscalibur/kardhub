//! Wasm ↔ JavaScript bridge for communicating with the background service worker.
//!
//! Provides typed async helpers that wrap `chrome.runtime.sendMessage` calls,
//! sending requests to the background worker and receiving deserialized responses.
//!
//! Uses a JS Promise wrapper around `chrome.runtime.sendMessage` so that
//! `wasm_bindgen_futures::JsFuture` can await the callback-based API.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// ── JS interop ───────────────────────────────────────────────────────

#[wasm_bindgen(module = "/src/bridge_glue.js")]
extern "C" {
    /// Send a message to the background worker, returning a Promise.
    #[wasm_bindgen(js_name = sendMessage)]
    fn js_send_message(msg: JsValue) -> js_sys::Promise;
}

/// Send a typed message to the background worker and await the response.
///
/// Takes a `serde_json::Value`, converts it to `JsValue`, sends it via
/// the JS glue, and returns the raw `JsValue` response.
pub async fn send_message(msg: &serde_json::Value) -> Result<JsValue, String> {
    let js_msg = js_sys::JSON::parse(&serde_json::to_string(msg).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{e:?}"))?;
    let promise = js_send_message(js_msg);
    JsFuture::from(promise).await.map_err(|e| format!("{e:?}"))
}

// ── Typed request helpers ────────────────────────────────────────────

/// Check whether a token is configured.
pub async fn check_token() -> Result<bool, String> {
    let msg = serde_json::json!({ "type": "getToken" });
    let resp = send_message(&msg).await?;
    let ok = js_sys::Reflect::get(&resp, &"ok".into())
        .map(|v| v.as_bool().unwrap_or(false))
        .unwrap_or(false);
    if !ok {
        return Ok(false);
    }
    let data = js_sys::Reflect::get(&resp, &"data".into()).map_err(|e| format!("{e:?}"))?;
    let has_token = js_sys::Reflect::get(&data, &"hasToken".into())
        .map(|v| v.as_bool().unwrap_or(false))
        .unwrap_or(false);
    Ok(has_token)
}

/// Fetch raw cards JSON for a repository from the background worker.
pub async fn fetch_cards_raw(owner: &str, repo: &str) -> Result<JsValue, String> {
    let msg = serde_json::json!({
        "type": "fetchCards",
        "owner": owner,
        "repo": repo,
    });
    let resp = send_message(&msg).await?;
    extract_data(resp)
}

/// Search issues via the background worker (returns raw items array).
pub async fn search_issues_raw(query: &str) -> Result<JsValue, String> {
    let msg = serde_json::json!({
        "type": "searchIssues",
        "query": query,
    });
    let resp = send_message(&msg).await?;
    extract_data(resp)
}

/// Fetch a single PR from the background worker.
pub async fn get_pr_raw(owner: &str, repo: &str, pr_number: u64) -> Result<JsValue, String> {
    let msg = serde_json::json!({
        "type": "getPr",
        "owner": owner,
        "repo": repo,
        "prNumber": pr_number,
    });
    let resp = send_message(&msg).await?;
    extract_data(resp)
}

/// List all repos for the authenticated user.
#[allow(dead_code)]
pub async fn list_repos_raw() -> Result<JsValue, String> {
    let msg = serde_json::json!({ "type": "listRepos" });
    let resp = send_message(&msg).await?;
    extract_data(resp)
}

/// Update the body of a PR.
pub async fn update_pr_body(
    owner: &str,
    repo: &str,
    pr_number: u64,
    body: &str,
) -> Result<(), String> {
    let msg = serde_json::json!({
        "type": "updatePrBody",
        "owner": owner,
        "repo": repo,
        "prNumber": pr_number,
        "body": body,
    });
    let resp = send_message(&msg).await?;
    let ok = js_sys::Reflect::get(&resp, &"ok".into())
        .map(|v| v.as_bool().unwrap_or(false))
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        let err = js_sys::Reflect::get(&resp, &"error".into())
            .map(|v| v.as_string().unwrap_or_else(|| "unknown error".to_string()))
            .unwrap_or_else(|_| "unknown error".to_string());
        Err(err)
    }
}

/// Extract the `data` field from a background response, or return the error.
fn extract_data(resp: JsValue) -> Result<JsValue, String> {
    let ok = js_sys::Reflect::get(&resp, &"ok".into())
        .map(|v| v.as_bool().unwrap_or(false))
        .unwrap_or(false);
    if ok {
        js_sys::Reflect::get(&resp, &"data".into()).map_err(|e| format!("{e:?}"))
    } else {
        let err = js_sys::Reflect::get(&resp, &"error".into())
            .map(|v| v.as_string().unwrap_or_else(|| "unknown error".to_string()))
            .unwrap_or_else(|_| "unknown error".to_string());
        Err(err)
    }
}
