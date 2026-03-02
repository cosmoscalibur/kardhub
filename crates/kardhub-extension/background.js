// KardHub extension — background service worker.
//
// Proxies GitHub API requests from the content script (wasm) to avoid
// CORS restrictions. Reads the PAT from chrome.storage.local.
// Also loads the WASM module for card mapping and repo classification.

import init, { map_cards_json, classify_repos_json } from "./bridge/kardhub_wasm_bridge.js";

let wasmReady = null;

/** Ensure WASM module is initialised (lazy, once). */
async function ensureWasm() {
    if (!wasmReady) {
        wasmReady = init();
    }
    return wasmReady;
}

const API_BASE = "https://api.github.com";
const HEADERS = {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
};

/**
 * Read the stored GitHub PAT from extension storage.
 * @returns {Promise<string|null>}
 */
async function getToken() {
    const data = await chrome.storage.local.get("github_pat");
    return data.github_pat || null;
}

/**
 * Make an authenticated GET request to the GitHub API.
 * @param {string} path - API path (e.g. "/user")
 * @param {string} token - GitHub PAT
 * @returns {Promise<object>} Parsed JSON response
 */
async function apiGet(path, token) {
    const resp = await fetch(`${API_BASE}${path}`, {
        headers: { ...HEADERS, Authorization: `Bearer ${token}` },
    });
    if (!resp.ok) {
        const text = await resp.text();
        throw new Error(`GitHub API ${resp.status}: ${text}`);
    }
    return resp.json();
}

/**
 * Make an authenticated PATCH request to the GitHub API.
 * @param {string} path - API path
 * @param {string} token - GitHub PAT
 * @param {object} body - JSON body
 * @returns {Promise<object>} Parsed JSON response
 */
async function apiPatch(path, token, body) {
    const resp = await fetch(`${API_BASE}${path}`, {
        method: "PATCH",
        headers: {
            ...HEADERS,
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
    });
    if (!resp.ok) {
        const text = await resp.text();
        throw new Error(`GitHub API ${resp.status}: ${text}`);
    }
    return resp.json();
}

/**
 * Paginate a GitHub API endpoint collecting all pages.
 * @param {string} path - API path with query params
 * @param {string} token - GitHub PAT
 * @returns {Promise<Array>}
 */
async function apiPaginate(path, token) {
    const all = [];
    let page = 1;
    // eslint-disable-next-line no-constant-condition
    while (true) {
        const sep = path.includes("?") ? "&" : "?";
        const items = await apiGet(`${path}${sep}per_page=100&page=${page}`, token);
        if (!Array.isArray(items) || items.length === 0) break;
        all.push(...items);
        page++;
    }
    return all;
}

// Message handler — dispatches requests from the content script.
chrome.runtime.onMessage.addListener((request, _sender, sendResponse) => {
    handleMessage(request)
        .then((result) => sendResponse({ ok: true, data: result }))
        .catch((err) => sendResponse({ ok: false, error: err.message }));
    // Return true to keep the message channel open for async response.
    return true;
});

/**
 * Route incoming messages to the appropriate handler.
 * @param {object} msg - Message with `type` and optional params
 * @returns {Promise<object>}
 */
async function handleMessage(msg) {
    const token = await getToken();

    switch (msg.type) {
        case "getToken":
            return { hasToken: !!token };

        case "validateToken": {
            if (!token) throw new Error("No token configured");
            return apiGet("/user", token);
        }

        case "fetchCards": {
            if (!token) throw new Error("No token configured");
            const { owner, repo } = msg;
            const [issues, prs, closedIssues, closedPrs] = await Promise.all([
                apiPaginate(
                    `/repos/${owner}/${repo}/issues?state=open`,
                    token
                ).then((items) => items.filter((i) => !i.pull_request)),
                apiPaginate(
                    `/repos/${owner}/${repo}/pulls?state=open&sort=updated&direction=desc`,
                    token
                ),
                // Recently closed issues (latest 100)
                apiGet(
                    `/repos/${owner}/${repo}/issues?state=closed&sort=updated&direction=desc&per_page=100`,
                    token
                ).then((items) => (items || []).filter((i) => !i.pull_request)),
                // Recently closed/merged PRs (latest 100)
                apiGet(
                    `/repos/${owner}/${repo}/pulls?state=closed&sort=updated&direction=desc&per_page=100`,
                    token
                ),
            ]);
            return {
                issues: [...issues, ...closedIssues],
                prs: [...prs, ...(closedPrs || [])],
            };
        }

        case "fetchClosedIssues": {
            if (!token) throw new Error("No token configured");
            const { owner, repo } = msg;
            return apiGet(
                `/repos/${owner}/${repo}/issues?state=closed&sort=updated&direction=desc&per_page=100`,
                token
            );
        }

        case "fetchClosedPrs": {
            if (!token) throw new Error("No token configured");
            const { owner, repo } = msg;
            return apiGet(
                `/repos/${owner}/${repo}/pulls?state=closed&sort=updated&direction=desc&per_page=100`,
                token
            );
        }

        case "searchIssues": {
            if (!token) throw new Error("No token configured");
            const { query } = msg;
            const encoded = encodeURIComponent(query);
            return apiGet(`/search/issues?q=${encoded}&per_page=30`, token);
        }

        case "listOrgs": {
            if (!token) throw new Error("No token configured");
            return apiPaginate("/user/orgs", token);
        }

        case "listRepos": {
            if (!token) throw new Error("No token configured");
            return apiPaginate("/user/repos?type=all&sort=updated", token);
        }

        case "listOrgRepos": {
            if (!token) throw new Error("No token configured");
            const { org } = msg;
            return apiPaginate(`/orgs/${org}/repos?sort=updated`, token);
        }

        case "updatePrBody": {
            if (!token) throw new Error("No token configured");
            const { owner, repo, prNumber, body } = msg;
            return apiPatch(`/repos/${owner}/${repo}/pulls/${prNumber}`, token, {
                body,
            });
        }

        case "getPr": {
            if (!token) throw new Error("No token configured");
            const { owner, repo, prNumber } = msg;
            return apiGet(`/repos/${owner}/${repo}/pulls/${prNumber}`, token);
        }

        case "mapCards": {
            await ensureWasm();
            const { rawJson, owner, repo } = msg;
            return JSON.parse(map_cards_json(rawJson, owner, repo));
        }

        case "classifyRepos": {
            await ensureWasm();
            const { reposJson, orgsJson } = msg;
            return JSON.parse(classify_repos_json(reposJson, orgsJson));
        }

        default:
            throw new Error(`Unknown message type: ${msg.type}`);
    }
}
