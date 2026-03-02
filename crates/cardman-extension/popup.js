// Cardman extension — popup script.
//
// Manages GitHub PAT storage and validation via the extension popup.

const form = document.getElementById("token-form");
const input = document.getElementById("pat-input");
const saveBtn = document.getElementById("save-btn");
const clearBtn = document.getElementById("clear-btn");
const statusEl = document.getElementById("status");

/**
 * Show a status message with the given class.
 * @param {string} message
 * @param {"success"|"error"|"info"} type
 */
function showStatus(message, type) {
    statusEl.textContent = message;
    statusEl.className = `status status-${type}`;
}

/** Check if a token is already stored and display connection status. */
async function checkExisting() {
    const { github_pat } = await chrome.storage.local.get("github_pat");
    if (github_pat) {
        input.value = "";
        input.placeholder = "••••••••••••••";
        showStatus("✓ Token configured", "success");

        // Validate token in background
        chrome.runtime.sendMessage({ type: "validateToken" }, (resp) => {
            if (resp && resp.ok) {
                showStatus(`✓ Connected as ${resp.data.login}`, "success");
            } else {
                showStatus("⚠ Token may be invalid", "error");
            }
        });
    } else {
        showStatus("No token configured", "info");
    }
}

// Save token
form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const token = input.value.trim();
    if (!token) {
        showStatus("Please enter a token", "error");
        return;
    }

    saveBtn.disabled = true;
    saveBtn.textContent = "Saving…";

    await chrome.storage.local.set({ github_pat: token });
    input.value = "";
    input.placeholder = "••••••••••••••";

    // Validate
    chrome.runtime.sendMessage({ type: "validateToken" }, (resp) => {
        saveBtn.disabled = false;
        saveBtn.textContent = "Save";
        if (resp && resp.ok) {
            showStatus(`✓ Connected as ${resp.data.login}`, "success");
        } else {
            showStatus("⚠ Token saved but validation failed", "error");
        }
    });
});

// Clear token
clearBtn.addEventListener("click", async () => {
    await chrome.storage.local.remove("github_pat");
    input.value = "";
    input.placeholder = "ghp_...";
    showStatus("Token cleared", "info");
});

// Initialise on popup open
checkExisting();
