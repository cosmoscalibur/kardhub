// KardHub extension — content script loader.
//
// Runs on GitHub pages, injecting the KardHub dashboard tab and PR issue
// linker. Uses URL /issues#kardhub to toggle the Kanban view inline.

(function () {
    "use strict";

    // ── Page detection ────────────────────────────────────────────────────

    /** Detect the page type from the URL. */
    function detectPage() {
        const m = location.pathname.match(/^\/([^/]+)\/([^/]+)/);
        if (!m) return null;
        const owner = m[1];
        const repo = m[2];
        // Exclude GitHub system paths
        if (["settings", "marketplace", "explore", "notifications"].includes(owner))
            return null;

        const prMatch = location.pathname.match(/^\/[^/]+\/[^/]+\/pull\/(\d+)/);
        if (prMatch) {
            return { type: "pr", owner, repo, prNumber: parseInt(prMatch[1], 10) };
        }
        // Any repo sub-page is valid for the dashboard tab
        return { type: "repo", owner, repo };
    }

    /** Send a message to the background worker. */
    function bgMessage(msg) {
        return new Promise((resolve, reject) => {
            chrome.runtime.sendMessage(msg, (resp) => {
                if (chrome.runtime.lastError) {
                    reject(new Error(chrome.runtime.lastError.message));
                } else if (resp && resp.ok) {
                    resolve(resp.data);
                } else {
                    reject(new Error(resp?.error || "Unknown error"));
                }
            });
        });
    }


    // ── Dashboard tab + inline rendering ─────────────────────────────────

    let dashboardOpen = false;

    /** Find the repository navigation tab container. */
    function findNavBody() {
        return (
            document.querySelector(".UnderlineNav-body") ||
            document.querySelector("nav.UnderlineNav > ul") ||
            document.querySelector("nav[aria-label*='Repository'] ul") ||
            document.querySelector("[role='tablist']") ||
            document.getElementById("code-tab")?.closest("ul") ||
            document.querySelector("a[data-tab-item='code']")?.closest("ul") ||
            document.querySelector("a[data-tab-item='code']")?.parentElement?.parentElement
        );
    }

    /** Inject the KardHub tab aligned with other GitHub tabs. */
    function injectDashboardTab(page) {
        if (document.getElementById("kardhub-tab")) return;

        const navBody = findNavBody();
        if (!navBody) return;

        const tab = document.createElement("span");
        tab.id = "kardhub-tab";
        tab.className = "UnderlineNav-item";
        tab.style.cursor = "pointer";
        tab.setAttribute("role", "tab");
        tab.setAttribute("tabindex", "0");
        tab.setAttribute("data-tab-item", "kardhub");
        tab.innerHTML =
            '<svg class="octicon UnderlineNav-octicon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16">' +
            '<path fill="currentColor" d="M1.75 0h12.5C15.216 0 16 .784 16 1.75v12.5A1.75 1.75 0 0 1 14.25 16H1.75A1.75 1.75 0 0 1 0 14.25V1.75C0 .784.784 0 1.75 0ZM1.5 1.75v12.5c0 .138.112.25.25.25H4.5v-13H1.75a.25.25 0 0 0-.25.25ZM6 14.5h4v-13H6Zm5.5 0h2.75a.25.25 0 0 0 .25-.25V1.75a.25.25 0 0 0-.25-.25H11.5Z"/>' +
            '</svg>' +
            ' <span data-content="KardHub">KardHub</span>';

        const issuesPath = `/${page.owner}/${page.repo}/issues`;

        tab.addEventListener("click", (e) => {
            e.preventDefault();
            e.stopPropagation();
            e.stopImmediatePropagation();

            if (dashboardOpen) {
                closeDashboard();
                return;
            }

            // If already on the issues page, open inline
            if (location.pathname.replace(/\/$/, "") === issuesPath) {
                openDashboard(page);
            } else {
                // Navigate to issues page — hash triggers auto-open on load
                location.assign(issuesPath + "#kardhub");
            }
        });

        navBody.appendChild(tab);

        // Close dashboard when any GitHub tab is clicked
        navBody.querySelectorAll("a").forEach((ghTab) => {
            if (ghTab.id === "kardhub-tab") return;
            ghTab.addEventListener("click", () => {
                if (dashboardOpen) closeDashboard();
            });
        });

        // Auto-open if URL has #kardhub hash (e.g. navigated from another page)
        if (location.hash === "#kardhub") {
            setTimeout(() => openDashboard(page), 300);
        }
    }

    /** Open the dashboard inline — hides GitHub content via CSS data attr. */
    function openDashboard(page) {
        const tab = document.getElementById("kardhub-tab");
        if (!tab || dashboardOpen) return;

        // CSS attribute hides GitHub content containers
        document.body.setAttribute("data-kardhub-open", "");

        // Mark KardHub tab as active
        tab.setAttribute("aria-current", "page");

        // Create or show the dashboard container (inline, under the tabs)
        let container = document.getElementById("kardhub-dashboard");
        if (!container) {
            container = document.createElement("div");
            container.id = "kardhub-dashboard";
            // Insert as direct child of body — outside any content container
            // that CSS rules might hide
            document.body.appendChild(container);
        }
        container.style.display = "";
        container.innerHTML =
            '<div class="kardhub-loading"><div class="kardhub-spinner"></div>Loading board…</div>';

        if (location.hash !== "#kardhub") {
            history.replaceState(null, "", `${location.pathname}#kardhub`);
        }
        dashboardOpen = true;

        loadDashboard(container, page);
    }

    /** Close the dashboard, restore GitHub content. */
    function closeDashboard() {
        dashboardOpen = false;

        // Remove CSS attribute — GitHub content reappears
        document.body.removeAttribute("data-kardhub-open");

        const tab = document.getElementById("kardhub-tab");
        if (tab) tab.removeAttribute("aria-current");

        const container = document.getElementById("kardhub-dashboard");
        if (container) container.style.display = "none";

        // Remove hash
        history.replaceState(null, "", location.pathname + location.search);
    }

    // ── Multi-repo dashboard ─────────────────────────────────────────────

    let isOrgContext = false;
    let currentOrg = "";
    let selectedRepos = [];
    let classifiedData = null;
    let dataLoaded = false;

    /** Load dashboard data (classified via WASM) and render. */
    async function loadDashboard(container, page) {
        try {
            const hasToken = await bgMessage({ type: "getToken" });
            if (!hasToken?.hasToken) {
                container.innerHTML =
                    '<div class="kardhub-error">No GitHub token configured. Click the KardHub extension icon to set up.</div>';
                return;
            }

            if (selectedRepos.length === 0) {
                selectedRepos = [{ owner: page.owner, repo: page.repo }];
            }

            if (!dataLoaded) {
                const [orgsData, reposData] = await Promise.all([
                    bgMessage({ type: "listOrgs" }).catch(() => []),
                    bgMessage({ type: "listRepos" }).catch(() => []),
                ]);

                // Classify repos via WASM in the background worker
                classifiedData = await bgMessage({
                    type: "classifyRepos",
                    reposJson: JSON.stringify(reposData || []),
                    orgsJson: JSON.stringify(orgsData || []),
                });
                dataLoaded = true;

                // Auto-detect context from current repo owner
                isOrgContext = classifiedData.orgs.some(
                    (o) => o.toLowerCase() === page.owner.toLowerCase()
                );
                if (isOrgContext) {
                    currentOrg = page.owner;
                }
            }

            renderDashboard(container, page);
        } catch (err) {
            container.innerHTML = `<div class="kardhub-error">${err.message}</div>`;
        }
    }

    /** Render the full dashboard: sidebar + board. */
    function renderDashboard(container, page) {
        container.innerHTML = "";

        const layout = document.createElement("div");
        layout.className = "kardhub-layout";

        // ── Left sidebar filter panel ──
        const sidebar = document.createElement("div");
        sidebar.className = "kardhub-sidebar";

        if (isOrgContext) {
            // Organization context: org dropdown + repos for selected org
            if (classifiedData.orgs.length > 1) {
                const orgSelect = document.createElement("select");
                orgSelect.className = "kardhub-org-select";
                classifiedData.orgs.forEach((org) => {
                    const opt = document.createElement("option");
                    opt.value = org;
                    opt.textContent = org;
                    opt.selected = org === currentOrg;
                    orgSelect.appendChild(opt);
                });
                orgSelect.addEventListener("change", () => {
                    currentOrg = orgSelect.value;
                    selectedRepos = [];
                    renderDashboard(container, page);
                });
                sidebar.appendChild(orgSelect);
            } else {
                const orgLabel = document.createElement("div");
                orgLabel.className = "kardhub-sidebar-title";
                orgLabel.textContent = `🏢 ${currentOrg}`;
                sidebar.appendChild(orgLabel);
            }

            const orgRepos = (classifiedData.orgRepos[currentOrg] || []);
            renderRepoCheckboxes(sidebar, orgRepos, container, page);
        } else {
            // Personal context: show personal repos directly
            renderRepoCheckboxes(sidebar, classifiedData.personalRepos, container, page);
        }

        layout.appendChild(sidebar);

        // ── Board area ──
        const boardArea = document.createElement("div");
        boardArea.className = "kardhub-board-area";
        const board = document.createElement("div");
        board.className = "kardhub-board";
        board.id = "kardhub-board";
        boardArea.appendChild(board);
        layout.appendChild(boardArea);

        container.appendChild(layout);

        loadBoard();
    }

    /** Render repo checkboxes inside sidebar. */
    function renderRepoCheckboxes(sidebar, repos, container, page) {
        const title = document.createElement("div");
        title.className = "kardhub-sidebar-title";
        title.textContent = "Repositories";
        sidebar.appendChild(title);

        const list = document.createElement("div");
        list.className = "kardhub-repo-list";

        repos.forEach((r) => {
            const label = document.createElement("label");
            label.className = "kardhub-repo-item";

            const cb = document.createElement("input");
            cb.type = "checkbox";
            cb.checked = selectedRepos.some(
                (s) =>
                    s.owner.toLowerCase() === r.owner.toLowerCase() &&
                    s.repo.toLowerCase() === r.repo.toLowerCase()
            );
            cb.addEventListener("change", () => {
                if (cb.checked) {
                    selectedRepos.push({ owner: r.owner, repo: r.repo });
                } else {
                    selectedRepos = selectedRepos.filter(
                        (s) =>
                            !(
                                s.owner.toLowerCase() === r.owner.toLowerCase() &&
                                s.repo.toLowerCase() === r.repo.toLowerCase()
                            )
                    );
                }
                loadBoard();
            });

            const span = document.createElement("span");
            span.textContent = r.repo;

            label.appendChild(cb);
            label.appendChild(span);
            list.appendChild(label);
        });

        sidebar.appendChild(list);
    }

    /** Load and render cards for all selected repos via WASM mapping. */
    async function loadBoard() {
        const boardEl = document.getElementById("kardhub-board");
        if (!boardEl) return;

        if (selectedRepos.length === 0) {
            boardEl.innerHTML =
                '<div class="kardhub-hint">Select one or more repositories</div>';
            return;
        }

        boardEl.innerHTML =
            '<div class="kardhub-loading"><div class="kardhub-spinner"></div>Loading…</div>';

        try {
            // Fetch raw data and map each repo through WASM via background
            const allMapped = [];
            let columns = [];

            const results = await Promise.all(
                selectedRepos.map((r) =>
                    bgMessage({ type: "fetchCards", owner: r.owner, repo: r.repo })
                        .then((d) => ({ data: d, owner: r.owner, repo: r.repo }))
                        .catch((err) => {
                            console.warn(`[KardHub] Failed to fetch ${r.owner}/${r.repo}:`, err.message);
                            return { data: { issues: [], prs: [] }, owner: r.owner, repo: r.repo };
                        })
                )
            );

            // Map each result through WASM in the background worker
            for (const { data, owner, repo } of results) {
                const mapped = await bgMessage({
                    type: "mapCards",
                    rawJson: JSON.stringify(data),
                    owner,
                    repo,
                });
                if (mapped.columns && mapped.columns.length > 0) {
                    columns = mapped.columns;
                }
                allMapped.push(...(mapped.cards || []));
            }

            renderBoard(boardEl, columns, allMapped);
        } catch (err) {
            boardEl.innerHTML = `<div class="kardhub-error">${err.message}</div>`;
        }
    }

    /** Render Kanban columns and cards from WASM-mapped data. */
    function renderBoard(boardEl, columns, cards) {
        boardEl.innerHTML = "";
        const multiRepo = selectedRepos.length > 1;

        // Group cards by column name
        const columnMap = {};
        columns.forEach((c) => (columnMap[c.name] = []));
        cards.forEach((card) => {
            if (columnMap[card.column]) {
                columnMap[card.column].push(card);
            }
        });

        columns.forEach((colDef) => {
            const colCards = columnMap[colDef.name] || [];
            const col = document.createElement("div");
            col.className = "kardhub-column";

            const hdr = document.createElement("div");
            hdr.className = "kardhub-column-header";
            hdr.innerHTML = `
        <span class="kardhub-column-emoji">${colDef.emoji}</span>
        <span class="kardhub-column-name">${colDef.name}</span>
        <span class="kardhub-column-count">${colCards.length}</span>`;
            col.appendChild(hdr);

            const cardList = document.createElement("div");
            cardList.className = "kardhub-column-cards";

            colCards.forEach((c) => {
                const icon = c.isPr ? "🔀" : "📋";
                const path = c.isPr ? "pull" : "issues";
                const url = `https://github.com/${c.owner}/${c.repo}/${path}/${c.number}`;

                const card = document.createElement("a");
                card.className = "kardhub-card";
                card.href = url;
                card.target = "_blank";
                card.rel = "noopener";

                const repoTag = multiRepo
                    ? `<span class="kardhub-card-repo">${c.owner}/${c.repo}</span>`
                    : "";

                let assigneeHtml = "";
                if (c.assignees && c.assignees.length > 0) {
                    assigneeHtml = '<div class="kardhub-card-assignees">';
                    c.assignees.forEach((a) => {
                        if (a.avatar_url) {
                            assigneeHtml += `<img class="kardhub-card-avatar" src="${a.avatar_url}&s=20" alt="${a.login}" title="${a.login}">`;
                        } else {
                            assigneeHtml += `<span class="kardhub-card-avatar-fallback" title="${a.login}">${(a.login || "?").charAt(0).toUpperCase()}</span>`;
                        }
                    });
                    assigneeHtml += "</div>";
                }

                let labelsHtml = "";
                if (c.labels && c.labels.length > 0) {
                    labelsHtml = '<div class="kardhub-card-labels">';
                    c.labels.forEach((l) => {
                        labelsHtml += `<span class="kardhub-label" style="background:#${l.color}">${l.name}</span>`;
                    });
                    labelsHtml += "</div>";
                }

                card.innerHTML = `
          <div class="kardhub-card-header">
            <span class="kardhub-card-icon">${icon}</span>
            <span class="kardhub-card-number">#${c.number}</span>
            ${repoTag}
          </div>
          <div class="kardhub-card-title">${c.title}</div>
          ${assigneeHtml}
          ${labelsHtml}`;

                cardList.appendChild(card);
            });

            col.appendChild(cardList);
            boardEl.appendChild(col);
        });
    }

    // ── PR Issue Linker (under PR body, full width) ──────────────────────

    /** Inject the issue linker widget under the PR description body. */
    function injectIssueLinker(page) {
        if (document.getElementById("kardhub-linker-widget")) return;

        const prBodyComment =
            document.querySelector(".js-discussion .timeline-comment:first-of-type") ||
            document.querySelector(".js-discussion .js-comment-container:first-of-type");
        if (!prBodyComment) return;

        const widget = document.createElement("div");
        widget.id = "kardhub-linker-widget";
        widget.className = "kardhub-linker-widget";

        // Collapsible header
        const header = document.createElement("div");
        header.className = "kardhub-linker-header";
        header.innerHTML = "🃏 Link Issues";
        header.addEventListener("click", () => {
            body.style.display = body.style.display === "none" ? "" : "none";
            header.classList.toggle("kardhub-linker-header-open");
        });

        const body = document.createElement("div");
        body.className = "kardhub-linker-body";
        body.style.display = "none";

        // Repo dropdown
        const repoRow = document.createElement("div");
        repoRow.className = "kardhub-linker-row";
        repoRow.innerHTML = "<label>Repository</label>";
        const repoSelect = document.createElement("select");
        repoSelect.className = "kardhub-input";
        const defaultOpt = document.createElement("option");
        defaultOpt.value = `${page.owner}/${page.repo}`;
        defaultOpt.textContent = `${page.owner}/${page.repo}`;
        defaultOpt.selected = true;
        repoSelect.appendChild(defaultOpt);
        repoRow.appendChild(repoSelect);
        body.appendChild(repoRow);

        // Populate repo dropdown — filter to current org/owner only
        bgMessage({ type: "listRepos" })
            .then((repos) => {
                (repos || []).forEach((r) => {
                    const full = r.full_name || `${r.owner?.login}/${r.name}`;
                    const owner = r.owner?.login || "";
                    // Only show repos from the same owner as the current repo
                    if (owner.toLowerCase() !== page.owner.toLowerCase()) return;
                    if (full === `${page.owner}/${page.repo}`) return;
                    const opt = document.createElement("option");
                    opt.value = full;
                    opt.textContent = full;
                    repoSelect.appendChild(opt);
                });
            })
            .catch(() => { });

        // Search input
        const searchRow = document.createElement("div");
        searchRow.className = "kardhub-linker-row";
        searchRow.innerHTML = "<label>Search issues</label>";
        const searchInput = document.createElement("input");
        searchInput.type = "text";
        searchInput.className = "kardhub-input";
        searchInput.placeholder = "Type to search…";
        searchRow.appendChild(searchInput);
        body.appendChild(searchRow);

        // Search button
        const searchBtnRow = document.createElement("div");
        searchBtnRow.className = "kardhub-linker-row";
        const searchBtn = document.createElement("button");
        searchBtn.className = "kardhub-btn kardhub-btn-primary";
        searchBtn.textContent = "Search";
        searchBtnRow.appendChild(searchBtn);
        body.appendChild(searchBtnRow);

        // Results
        const results = document.createElement("div");
        results.className = "kardhub-search-results";
        results.innerHTML =
            '<p class="kardhub-hint">Type a query and click Search</p>';
        body.appendChild(results);

        // Selected list
        const selectedRow = document.createElement("div");
        selectedRow.className = "kardhub-linker-row";
        selectedRow.innerHTML = "<label>Selected issues</label>";
        const selectedList = document.createElement("div");
        selectedList.className = "kardhub-selected-issues";
        selectedList.innerHTML =
            '<p class="kardhub-hint">No issues selected</p>';
        selectedRow.appendChild(selectedList);
        body.appendChild(selectedRow);

        // Action buttons
        const actionRow = document.createElement("div");
        actionRow.className = "kardhub-linker-actions";
        const addBtn = document.createElement("button");
        addBtn.className = "kardhub-btn kardhub-btn-primary";
        addBtn.textContent = "Add to PR body";
        addBtn.disabled = true;
        actionRow.appendChild(addBtn);
        body.appendChild(actionRow);

        const selected = [];

        function doSearch() {
            const query = searchInput.value.trim();
            const targetRepo = repoSelect.value;
            if (!query) return;
            searchBtn.textContent = "Searching…";
            searchBtn.disabled = true;

            bgMessage({
                type: "searchIssues",
                query: `repo:${targetRepo} is:issue ${query}`,
            })
                .then((data) => {
                    results.innerHTML = "";
                    const items = (data?.items || data || []).filter((i) => !i.pull_request);
                    if (items.length === 0) {
                        results.innerHTML =
                            '<p class="kardhub-hint">No results found</p>';
                        return;
                    }
                    items.forEach((item) => {
                        const already = selected.some(
                            (s) => s.number === item.number && s.repo === targetRepo
                        );
                        const row = document.createElement("div");
                        row.className = "kardhub-search-item";
                        row.innerHTML = `
              <span class="kardhub-search-number">#${item.number}</span>
              <span class="kardhub-search-title">${item.title}</span>`;
                        const btn = document.createElement("button");
                        btn.className = "kardhub-btn-add";
                        btn.textContent = already ? "✓" : "+";
                        btn.disabled = already;
                        btn.addEventListener("click", () => {
                            selected.push({
                                repo: targetRepo,
                                number: item.number,
                                title: item.title,
                            });
                            btn.textContent = "✓";
                            btn.disabled = true;
                            updateSelectedDisplay();
                        });
                        row.appendChild(btn);
                        results.appendChild(row);
                    });
                })
                .catch((err) => {
                    results.innerHTML = `<p class="kardhub-error-sm">${err.message}</p>`;
                })
                .finally(() => {
                    searchBtn.textContent = "Search";
                    searchBtn.disabled = false;
                });
        }

        searchBtn.addEventListener("click", doSearch);
        searchInput.addEventListener("keydown", (e) => {
            if (e.key === "Enter") doSearch();
        });

        function updateSelectedDisplay() {
            selectedList.innerHTML = "";
            if (selected.length === 0) {
                selectedList.innerHTML =
                    '<p class="kardhub-hint">No issues selected</p>';
                addBtn.disabled = true;
                return;
            }
            addBtn.disabled = false;
            selected.forEach((s, i) => {
                const row = document.createElement("div");
                row.className = "kardhub-selected-item";
                row.innerHTML = `<span>Issue: ${s.repo}#${s.number}</span>`;
                const rm = document.createElement("button");
                rm.className = "kardhub-btn-remove";
                rm.textContent = "✕";
                rm.addEventListener("click", () => {
                    selected.splice(i, 1);
                    updateSelectedDisplay();
                });
                row.appendChild(rm);
                selectedList.appendChild(row);
            });
        }

        addBtn.addEventListener("click", async () => {
            addBtn.textContent = "Updating…";
            addBtn.disabled = true;
            try {
                const prData = await bgMessage({
                    type: "getPr",
                    owner: page.owner,
                    repo: page.repo,
                    prNumber: page.prNumber,
                });
                const currentBody = prData?.body || "";
                const issueLines = selected
                    .map((s) => `Issue: ${s.repo}#${s.number}`)
                    .join("\n");
                const newBody = currentBody.includes("## Issues")
                    ? `${currentBody}\n${issueLines}`
                    : `${currentBody}\n\n## Issues\n${issueLines}`;

                await bgMessage({
                    type: "updatePrBody",
                    owner: page.owner,
                    repo: page.repo,
                    prNumber: page.prNumber,
                    body: newBody,
                });
                location.reload();
            } catch (err) {
                const errP = document.createElement("p");
                errP.className = "kardhub-error-sm";
                errP.textContent = err.message;
                actionRow.appendChild(errP);
            } finally {
                addBtn.textContent = "Add to PR body";
                addBtn.disabled = selected.length === 0;
            }
        });

        widget.appendChild(header);
        widget.appendChild(body);

        prBodyComment.parentElement.insertBefore(
            widget,
            prBodyComment.nextSibling
        );
    }

    // ── Init & SPA navigation ───────────────────────────────────────────

    let lastUrl = location.href;

    function init() {
        dashboardOpen = false;
        document.body.removeAttribute("data-kardhub-open");

        const page = detectPage();
        if (!page) return;

        tryInjectTab(page, 0);
    }

    /** Retry tab injection until nav element appears (max 20 attempts). */
    function tryInjectTab(page, attempt) {
        if (document.getElementById("kardhub-tab")) return;

        const navBody = findNavBody();

        if (!navBody) {
            if (attempt < 20) {
                setTimeout(() => tryInjectTab(page, attempt + 1), 500);
            }
            return;
        }

        injectDashboardTab(page);
        if (page.type === "pr") {
            injectIssueLinker(page);
        }
    }

    init();

    /** Clean up injected elements and reset state. */
    function cleanup() {
        dashboardOpen = false;
        document.body.removeAttribute("data-kardhub-open");
        ["kardhub-tab", "kardhub-dashboard", "kardhub-linker-widget"].forEach(
            (id) => {
                const el = document.getElementById(id);
                if (el) el.remove();
            }
        );
        selectedRepos = [];
        classifiedData = null;
        dataLoaded = false;
    }

    // Watch for SPA navigation via MutationObserver
    const observer = new MutationObserver(() => {
        if (location.href !== lastUrl) {
            lastUrl = location.href;
            cleanup();
            setTimeout(init, 300);
        }
    });

    observer.observe(document.body, { childList: true, subtree: true });

    // Also listen for Turbo (GitHub's SPA framework) navigation
    document.addEventListener("turbo:load", () => {
        if (location.href !== lastUrl) {
            lastUrl = location.href;
            cleanup();
        }
        // Re-inject if tab is missing (Turbo may replace the nav)
        setTimeout(init, 100);
    });
})();
