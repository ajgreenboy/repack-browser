const API_BASE = '/api';
let currentPage = 1;
let selectedGameId = null;
let searchTimeout = null;
let statusCheckInterval = null;
let downloadPollInterval = null;
let currentView = 'games'; // 'games' or 'downloads'
let favoriteIds = new Set();
let showingFavorites = false;

// Load games on page load
document.addEventListener('DOMContentLoaded', () => {
    loadGames();
    loadGenres();
    loadFavoriteIds();
    // Check if a scrape is already running (e.g. page refresh during scrape)
    checkScrapeStatus().then(() => {
        const container = document.getElementById('scrapeProgressContainer');
        if (container && !container.classList.contains('hidden')) {
            statusCheckInterval = setInterval(checkScrapeStatus, 2000);
        }
    });
});

// ─── View switching ───

function showView(view) {
    currentView = view;
    document.getElementById('gamesView').classList.toggle('hidden', view !== 'games');
    document.getElementById('downloadsView').classList.toggle('hidden', view !== 'downloads');
    document.getElementById('navGames').classList.toggle('active', view === 'games');
    document.getElementById('navDownloads').classList.toggle('active', view === 'downloads');

    if (view === 'downloads') {
        loadDownloads();
        startDownloadPolling();
    } else {
        stopDownloadPolling();
    }
}

// ─── Games ───

async function loadGames(page = 1) {
    currentPage = page;
    const search = document.getElementById('searchInput').value;
    const sort = document.getElementById('sortSelect').value;
    const genre = document.getElementById('genreSelect').value;

    const params = new URLSearchParams({
        page: currentPage,
        per_page: 30
    });

    if (search) params.append('search', search);
    if (sort) params.append('sort', sort);
    if (genre) params.append('genre', genre);

    showLoading(true);
    hideError();

    try {
        const response = await fetch(`${API_BASE}/games?${params}`);
        if (!response.ok) throw new Error('Failed to load games');

        const data = await response.json();
        currentGames = data.games || [];
        renderGames(data.games);
        renderPagination(data);
        updateStats(data);

        if (data.total === 0) {
            document.getElementById('emptyState').classList.remove('hidden');
        } else {
            document.getElementById('emptyState').classList.add('hidden');
        }
    } catch (error) {
        showError('Failed to load games. Please try again.');
        console.error('Error loading games:', error);
    } finally {
        showLoading(false);
    }
}

function showLoading(show) {
    document.getElementById('loadingIndicator').classList.toggle('hidden', !show);
    document.getElementById('gamesGrid').classList.toggle('hidden', show);
}

function showError(message) {
    document.getElementById('errorText').textContent = message;
    document.getElementById('errorBanner').classList.remove('hidden');
}

function hideError() {
    document.getElementById('errorBanner').classList.add('hidden');
}

function renderGames(games) {
    const grid = document.getElementById('gamesGrid');

    if (games.length === 0) {
        grid.innerHTML = '';
        return;
    }

    grid.innerHTML = games.map(game => {
        const hasThumb = game.thumbnail_url && game.thumbnail_url.length > 0;
        const isFav = favoriteIds.has(game.id);
        const thumb = hasThumb
            ? `<div class="game-thumb"><img src="${escapeHtml(game.thumbnail_url)}" alt="" loading="lazy" onerror="this.parentElement.classList.add('game-thumb-fallback');this.remove()"><button onclick="event.stopPropagation();toggleFavorite(${game.id})" class="fav-star" title="${isFav ? 'Remove from favorites' : 'Add to favorites'}">${isFav ? '⭐' : '☆'}</button></div>`
            : `<div class="game-thumb game-thumb-fallback"><span>🎮</span><button onclick="event.stopPropagation();toggleFavorite(${game.id})" class="fav-star" title="${isFav ? 'Remove from favorites' : 'Add to favorites'}">${isFav ? '⭐' : '☆'}</button></div>`;

        const genres = game.genres
            ? `<div class="card-genres">${escapeHtml(game.genres).split(',').map(g => {
                const trimmed = g.trim();
                return `<span onclick="event.stopPropagation();filterByGenre('${trimmed}')" class="genre-tag">${trimmed}</span>`;
            }).join('')}</div>`
            : '';

        const company = game.company
            ? `<div class="card-company">${escapeHtml(game.company)}</div>`
            : '';

        const year = game.post_date ? game.post_date.substring(0, 4) : '';
        const yearBadge = year ? `<span class="card-year">${year}</span>` : '';

        const sizes = game.original_size
            ? `${escapeHtml(game.file_size)} <span class="arrow">→</span> ${escapeHtml(game.original_size)}`
            : `${escapeHtml(game.file_size)}`;

        return `
            <div class="game-card ${isFav ? 'favorited' : ''}" onclick="showGameModal(${game.id})">
                ${thumb}
                <div class="card-body">
                    <div class="card-header">
                        <h3 class="card-title">${escapeHtml(game.title)}</h3>
                        ${yearBadge}
                    </div>
                    ${company}
                    ${genres}
                    <div class="card-size">${sizes}</div>
                </div>
            </div>
        `;
    }).join('');
}

function renderPagination(data) {
    const pagination = document.getElementById('pagination');
    const { page, total_pages } = data;

    if (total_pages <= 1) {
        pagination.innerHTML = '';
        return;
    }

    let html = '';

    if (page > 1) {
        html += `<button onclick="loadGames(${page - 1})" class="page-btn">← Prev</button>`;
    }

    const startPage = Math.max(1, page - 2);
    const endPage = Math.min(total_pages, page + 2);

    if (startPage > 1) {
        html += `<button onclick="loadGames(1)" class="page-btn">1</button>`;
        if (startPage > 2) html += `<span class="page-ellipsis">…</span>`;
    }

    for (let i = startPage; i <= endPage; i++) {
        const isActive = i === page;
        html += `<button onclick="loadGames(${i})" class="page-btn ${isActive ? 'active' : ''}">${i}</button>`;
    }

    if (endPage < total_pages) {
        if (endPage < total_pages - 1) html += `<span class="page-ellipsis">…</span>`;
        html += `<button onclick="loadGames(${total_pages})" class="page-btn">${total_pages}</button>`;
    }

    if (page < total_pages) {
        html += `<button onclick="loadGames(${page + 1})" class="page-btn">Next →</button>`;
    }

    pagination.innerHTML = html;
}

function updateStats(data) {
    const stats = document.getElementById('statsText');
    stats.textContent = `Showing ${data.games.length} of ${data.total} games (Page ${data.page} of ${data.total_pages})`;
}

function handleSearchChange() {
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
        currentPage = 1;
        loadGames();
    }, 500);
}

function clearFilters() {
    document.getElementById('searchInput').value = '';
    document.getElementById('sortSelect').value = '';
    document.getElementById('genreSelect').value = '';
    showingFavorites = false;
    document.getElementById('favToggle').classList.remove('bg-yellow-700');
    currentPage = 1;
    loadGames();
}

// ─── Game Modal with Screenshot Gallery ───

// Store current page games for modal lookup
let currentGames = [];
let modalScreenshotIndex = 0;
let modalScreenshots = [];

function showGameModal(gameId) {
    const game = currentGames.find(g => g.id === gameId);
    if (!game) return;

    selectedGameId = gameId;
    const isFav = favoriteIds.has(gameId);

    // Parse screenshots (stored as ||| separated URLs)
    modalScreenshots = game.screenshots ? game.screenshots.split('|||').filter(s => s.length > 0) : [];
    modalScreenshotIndex = 0;

    // If no screenshots, use thumbnail as the only image
    if (modalScreenshots.length === 0 && game.thumbnail_url) {
        modalScreenshots = [game.thumbnail_url];
    }

    const galleryHtml = modalScreenshots.length > 0 ? `
        <div class="relative mb-3 bg-gray-900 rounded overflow-hidden">
            <img id="modalScreenshot" src="${escapeHtml(modalScreenshots[0])}" alt="" class="w-full max-h-64 object-contain" onerror="this.style.display='none'">
            ${modalScreenshots.length > 1 ? `
                <button onclick="prevScreenshot()" class="absolute left-1 top-1/2 -translate-y-1/2 bg-black bg-opacity-60 hover:bg-opacity-80 text-white rounded-full w-8 h-8 flex items-center justify-center">‹</button>
                <button onclick="nextScreenshot()" class="absolute right-1 top-1/2 -translate-y-1/2 bg-black bg-opacity-60 hover:bg-opacity-80 text-white rounded-full w-8 h-8 flex items-center justify-center">›</button>
                <div class="absolute bottom-1 left-1/2 -translate-x-1/2 bg-black bg-opacity-60 text-white text-xs px-2 py-0.5 rounded">
                    <span id="screenshotCounter">1 / ${modalScreenshots.length}</span>
                </div>
            ` : ''}
        </div>
        ${modalScreenshots.length > 1 ? `
            <div class="flex gap-1 mb-3 overflow-x-auto pb-1">
                ${modalScreenshots.map((url, i) => `
                    <img src="${escapeHtml(url)}" alt="" onclick="goToScreenshot(${i})"
                         class="h-12 w-20 object-cover rounded cursor-pointer border-2 ${i === 0 ? 'border-blue-500' : 'border-transparent'} hover:border-blue-400 flex-shrink-0 screenshot-thumb"
                         onerror="this.style.display='none'"
                         data-index="${i}">
                `).join('')}
            </div>
        ` : ''}
    ` : '';

    const genres = game.genres
        ? `<div class="card-genres" style="margin-bottom:0.5rem">${escapeHtml(game.genres).split(',').map(g => {
            const trimmed = g.trim();
            return `<span onclick="hideConfirmModal();filterByGenre('${trimmed}')" class="genre-tag">${trimmed}</span>`;
        }).join('')}</div>`
        : '';
    const company = game.company
        ? `<p style="margin-bottom:0.375rem;font-size:0.85rem;color:var(--text-dim)">${escapeHtml(game.company)}</p>`
        : '';
    const origSize = game.original_size
        ? `<p style="margin-bottom:0.25rem;font-size:0.85rem"><strong>Original Size:</strong> ${escapeHtml(game.original_size)}</p>`
        : '';
    const sourceLink = game.source_url
        ? `<p style="margin-bottom:0.25rem"><a href="${escapeHtml(game.source_url)}" target="_blank" style="color:var(--accent-bright);font-size:0.85rem;text-decoration:none">View on FitGirl Repacks →</a></p>`
        : '';

    document.getElementById('confirmContent').innerHTML = `
        ${galleryHtml}
        <p style="margin-bottom:0.25rem;font-size:1.1rem;font-weight:700">${escapeHtml(game.title)}</p>
        ${company}
        ${genres}
        <p style="margin-bottom:0.25rem;font-size:0.85rem"><strong>Repack Size:</strong> ${escapeHtml(game.file_size)}</p>
        ${origSize}
        ${sourceLink}
    `;
    document.getElementById('confirmModal').classList.remove('hidden');

    const btnContainer = document.getElementById('confirmBtnContainer');
    btnContainer.innerHTML = `
        <button onclick="toggleFavorite(${gameId});updateModalFavBtn(${gameId})"
                id="modalFavBtn"
                class="btn ${isFav ? 'btn-gold' : 'btn-ghost'}">
            ${isFav ? '⭐ Favorited' : '☆ Favorite'}
        </button>
        <button id="rdLinksBtn" onclick="addToRealDebrid(${gameId})"
                class="btn btn-primary" style="flex:1">
            Get Links
        </button>
        <button onclick="hideConfirmModal()"
                class="btn btn-secondary" style="flex:0">
            Cancel
        </button>
    `;
}

function prevScreenshot() {
    modalScreenshotIndex = (modalScreenshotIndex - 1 + modalScreenshots.length) % modalScreenshots.length;
    updateScreenshot();
}

function nextScreenshot() {
    modalScreenshotIndex = (modalScreenshotIndex + 1) % modalScreenshots.length;
    updateScreenshot();
}

function goToScreenshot(index) {
    modalScreenshotIndex = index;
    updateScreenshot();
}

function updateScreenshot() {
    const img = document.getElementById('modalScreenshot');
    if (img) img.src = modalScreenshots[modalScreenshotIndex];
    const counter = document.getElementById('screenshotCounter');
    if (counter) counter.textContent = `${modalScreenshotIndex + 1} / ${modalScreenshots.length}`;
    // Update thumbnail borders
    document.querySelectorAll('.screenshot-thumb').forEach(thumb => {
        const i = parseInt(thumb.dataset.index);
        thumb.classList.toggle('border-blue-500', i === modalScreenshotIndex);
        thumb.classList.toggle('border-transparent', i !== modalScreenshotIndex);
    });
}

function updateModalFavBtn(gameId) {
    const btn = document.getElementById('modalFavBtn');
    if (!btn) return;
    const isFav = favoriteIds.has(gameId);
    btn.className = `btn ${isFav ? 'btn-gold' : 'btn-ghost'}`;
    btn.innerHTML = isFav ? '⭐ Favorited' : '☆ Favorite';
}

function hideConfirmModal() {
    document.getElementById('confirmModal').classList.add('hidden');
    selectedGameId = null;
}

// ─── Queue Download ───

async function queueDownload(gameId) {
    const downloadBtn = document.getElementById('downloadBtn');
    if (downloadBtn) {
        downloadBtn.disabled = true;
        downloadBtn.innerHTML = '<span class="spinner"></span> Queuing...';
    }

    try {
        const response = await fetch(`${API_BASE}/downloads`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ game_id: gameId })
        });

        const data = await response.json();

        if (data.success) {
            showToast('Added to download queue!', 'success');
            hideConfirmModal();
            // Switch to downloads view
            showView('downloads');
        } else {
            showToast(data.message || 'Failed to queue download', 'error');
            if (downloadBtn) {
                downloadBtn.disabled = false;
                downloadBtn.innerHTML = '📥 Download';
            }
        }
    } catch (error) {
        showToast('Error queuing download', 'error');
        if (downloadBtn) {
            downloadBtn.disabled = false;
            downloadBtn.innerHTML = '📥 Download';
        }
    }
}

// ─── Real-Debrid Links (kept for manual use) ───

async function addToRealDebrid(gameId) {
    const rdBtn = document.getElementById('rdLinksBtn');
    if (rdBtn) {
        rdBtn.disabled = true;
        rdBtn.innerHTML = '<span class="spinner"></span> Processing...';
    }

    try {
        const response = await fetch(`${API_BASE}/realdebrid/add`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ game_id: gameId })
        });

        const data = await response.json();

        if (data.success && data.downloads && data.downloads.length > 0) {
            showDownloadLinks(data.downloads, data.message);
        } else {
            showToast(data.message || 'No download links available', 'error');
            hideConfirmModal();
        }
    } catch (error) {
        showToast('Error processing with Real-Debrid', 'error');
        hideConfirmModal();
    }
}

function showDownloadLinks(downloads, message) {
    document.getElementById('confirmContent').innerHTML = `
        <h3 class="font-bold mb-3 text-green-400">✔ ${escapeHtml(message)}</h3>
        <div class="space-y-2 max-h-96 overflow-y-auto">
            ${downloads.map(dl => `
                <a href="${escapeHtml(dl.download_url)}"
                   download
                   target="_blank"
                   class="block bg-gray-700 hover:bg-gray-600 p-3 rounded border border-gray-600 hover:border-green-500 transition">
                    <div class="font-semibold text-sm mb-1">📥 ${escapeHtml(dl.filename)}</div>
                    <div class="text-xs text-gray-400">Click to download</div>
                </a>
            `).join('')}
        </div>
        <p class="text-xs text-gray-400 mt-4">Direct download links from Real-Debrid. Links are valid for a limited time.</p>
    `;

    document.getElementById('confirmBtnContainer').innerHTML = `
        <button onclick="hideConfirmModal()" class="flex-1 bg-gray-600 hover:bg-gray-700 px-4 py-2 rounded">
            Close
        </button>
    `;

    showToast('Download links ready!', 'success');
}

// ─── Downloads View ───

function startDownloadPolling() {
    stopDownloadPolling();
    downloadPollInterval = setInterval(loadDownloads, 2000);
}

function stopDownloadPolling() {
    if (downloadPollInterval) {
        clearInterval(downloadPollInterval);
        downloadPollInterval = null;
    }
}

async function loadDownloads() {
    try {
        const response = await fetch(`${API_BASE}/downloads`);
        if (!response.ok) throw new Error('Failed to load downloads');

        const data = await response.json();
        renderDownloads(data.downloads);

        // Update badge count
        const activeCount = data.downloads.filter(d =>
            d.status === 'queued' || d.status === 'downloading' || d.status === 'extracting'
        ).length;
        const badge = document.getElementById('downloadBadge');
        if (activeCount > 0) {
            badge.textContent = activeCount;
            badge.classList.remove('hidden');
        } else {
            badge.classList.add('hidden');
        }
    } catch (error) {
        console.error('Error loading downloads:', error);
    }
}

function renderDownloads(downloads) {
    const container = document.getElementById('downloadsList');

    if (downloads.length === 0) {
        container.innerHTML = `
            <div class="text-center py-16">
                <p class="text-2xl text-gray-500 mb-4">No downloads yet</p>
                <p class="text-gray-400">Click a game and hit "Download" to get started.</p>
            </div>
        `;
        return;
    }

    container.innerHTML = downloads.map(dl => {
        const statusIcon = getStatusIcon(dl.status);
        const statusColor = getStatusColor(dl.status);
        const progressPct = Math.min(100, Math.max(0, dl.progress || 0));

        let statsHtml = '';
        let actionsHtml = '';
        let extractPct = null;
        let extractInfo = null;

        switch (dl.status) {
            case 'queued':
                statsHtml = '<span class="text-yellow-400">Waiting in queue...</span>';
                actionsHtml = `<button onclick="cancelDownload(${dl.id})" class="text-sm bg-red-600 hover:bg-red-700 px-3 py-1 rounded">Cancel</button>`;
                break;

            case 'downloading':
                const speedStr = dl.download_speed || '—';
                const etaStr = dl.eta || '—';
                const progressStr = `${progressPct.toFixed(1)}%`;
                statsHtml = `
                    <span>${progressStr}</span>
                    <span class="mx-2">•</span>
                    <span>${speedStr}</span>
                    <span class="mx-2">•</span>
                    <span>${etaStr} remaining</span>
                `;
                actionsHtml = `<button onclick="cancelDownload(${dl.id})" class="text-sm bg-red-600 hover:bg-red-700 px-3 py-1 rounded">Cancel</button>`;
                break;

            case 'extracting':
                if (dl.extract_progress) {
                    const ep = dl.extract_progress;
                    const epPct = Math.min(100, Math.max(0, ep.percent || 0));
                    const filesInfo = ep.files_total > 0
                        ? `${ep.files_done}/${ep.files_total} files`
                        : `${ep.files_done} files`;
                    statsHtml = `
                        <span class="text-blue-400">${escapeHtml(ep.message)}</span>
                    `;
                    // Override progressPct for the bar
                    extractPct = epPct;
                    extractInfo = filesInfo;
                } else {
                    statsHtml = '<span class="text-blue-400">Extracting archives...</span>';
                    extractPct = null;
                    extractInfo = null;
                }
                break;

            case 'completed':
                statsHtml = `<span class="text-green-400">Ready to install${dl.completed_at ? ' • ' + formatDate(dl.completed_at) : ''}</span>`;
                actionsHtml = `
                    ${dl.installer_path ? `<button onclick="launchInstall(${dl.id})" class="text-sm bg-green-600 hover:bg-green-700 px-3 py-1 rounded font-semibold">🎮 Install</button>` : ''}
                    ${dl.file_path ? `<button onclick="copyPath('${escapeHtml(dl.file_path)}')" class="text-sm bg-gray-600 hover:bg-gray-500 px-3 py-1 rounded">📁 Open Folder</button>` : ''}
                    <button onclick="removeDownload(${dl.id})" class="text-sm bg-gray-700 hover:bg-gray-600 px-3 py-1 rounded">Remove</button>
                `;
                break;

            case 'installing':
                statsHtml = '<span class="text-purple-400">Installer launched — complete the setup wizard</span>';
                actionsHtml = `
                    <button onclick="markInstalled(${dl.id})" class="text-sm bg-green-600 hover:bg-green-700 px-3 py-1 rounded">✅ Done Installing</button>
                    <button onclick="launchInstall(${dl.id})" class="text-sm bg-gray-600 hover:bg-gray-500 px-3 py-1 rounded">🔄 Relaunch</button>
                `;
                break;

            case 'installed':
                statsHtml = `<span class="text-green-400">✅ Installed${dl.completed_at ? ' • ' + formatDate(dl.completed_at) : ''}</span>`;
                actionsHtml = `
                    ${dl.file_path ? `<button onclick="copyPath('${escapeHtml(dl.file_path)}')" class="text-sm bg-gray-600 hover:bg-gray-500 px-3 py-1 rounded">📁 Open Folder</button>` : ''}
                    <button onclick="removeDownload(${dl.id})" class="text-sm bg-gray-700 hover:bg-gray-600 px-3 py-1 rounded">Remove</button>
                `;
                break;

            case 'failed':
                statsHtml = `<span class="text-red-400">${escapeHtml(dl.error_message || 'Unknown error')}</span>`;
                actionsHtml = `
                    <button onclick="retryDownload(${dl.id})" class="text-sm bg-yellow-600 hover:bg-yellow-700 px-3 py-1 rounded">Retry</button>
                    <button onclick="removeDownload(${dl.id})" class="text-sm bg-gray-700 hover:bg-gray-600 px-3 py-1 rounded">Remove</button>
                `;
                break;
        }

        const showProgress = dl.status === 'downloading' || dl.status === 'extracting';
        const barPct = dl.status === 'extracting' && extractPct !== null ? extractPct : progressPct;
        const barColor = dl.status === 'extracting' ? 'bg-blue-500' : 'bg-green-500';
        const barAnimate = dl.status === 'extracting' && extractPct === null ? ' animate-pulse' : '';

        return `
            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <div class="flex justify-between items-start mb-2">
                    <div>
                        <h3 class="font-bold text-lg">${statusIcon} ${escapeHtml(dl.game_title)}</h3>
                        <p class="text-sm text-gray-400">${escapeHtml(dl.game_size)}</p>
                    </div>
                    <span class="text-xs px-2 py-1 rounded ${statusColor}">${dl.status.toUpperCase()}</span>
                </div>
                ${showProgress ? `
                    <div class="progress-bar-container bg-gray-700 rounded-full h-3 mb-1 overflow-hidden">
                        <div class="h-full rounded-full transition-all duration-300 ${barColor}${barAnimate}"
                             style="width: ${barPct}%"></div>
                    </div>
                    ${dl.status === 'extracting' && extractPct !== null ? `
                        <div class="flex justify-between text-xs text-gray-500 mb-2">
                            <span>${extractInfo || ''}</span>
                            <span>${extractPct.toFixed(1)}%</span>
                        </div>
                    ` : ''}
                ` : ''}
                <div class="flex justify-between items-center text-sm text-gray-400">
                    <div>${statsHtml}</div>
                    <div class="flex gap-2">${actionsHtml}</div>
                </div>
                ${dl.files && dl.files.length > 0 ? `
                    <details class="mt-2">
                        <summary class="text-xs text-gray-500 cursor-pointer hover:text-gray-300">
                            ${dl.files.length} file(s)
                        </summary>
                        <div class="mt-1 text-xs text-gray-500 space-y-1">
                            ${dl.files.map(f => `
                                <div class="flex justify-between">
                                    <span>${escapeHtml(f.filename)}</span>
                                    <span>${f.file_size ? formatBytes(f.file_size) : '—'}${f.is_extracted ? ' ✔' : ''}</span>
                                </div>
                            `).join('')}
                        </div>
                    </details>
                ` : ''}
            </div>
        `;
    }).join('');
}

// ─── Download Actions ───

async function cancelDownload(id) {
    if (!confirm('Cancel this download?')) return;

    try {
        const response = await fetch(`${API_BASE}/downloads/${id}`, { method: 'DELETE' });
        const data = await response.json();
        if (data.success) {
            showToast('Download cancelled', 'info');
            loadDownloads();
        } else {
            showToast(data.message, 'error');
        }
    } catch (error) {
        showToast('Error cancelling download', 'error');
    }
}

async function retryDownload(id) {
    try {
        const response = await fetch(`${API_BASE}/downloads/${id}/retry`, { method: 'POST' });
        const data = await response.json();
        if (data.success) {
            showToast('Download requeued', 'success');
            loadDownloads();
        } else {
            showToast(data.message, 'error');
        }
    } catch (error) {
        showToast('Error retrying download', 'error');
    }
}

async function removeDownload(id) {
    try {
        const response = await fetch(`${API_BASE}/downloads/${id}/remove`, { method: 'DELETE' });
        const data = await response.json();
        if (data.success) {
            showToast('Download removed', 'info');
            loadDownloads();
        } else {
            showToast(data.message, 'error');
        }
    } catch (error) {
        showToast('Error removing download', 'error');
    }
}

async function launchInstall(id) {
    try {
        const response = await fetch(`${API_BASE}/downloads/${id}/install`, { method: 'POST' });
        const data = await response.json();
        if (data.success) {
            showToast('Installer launched! Complete the setup wizard.', 'success');
            loadDownloads();
        } else {
            showToast(data.message, 'error');
        }
    } catch (error) {
        showToast('Error launching installer', 'error');
    }
}

async function markInstalled(id) {
    try {
        const response = await fetch(`${API_BASE}/downloads/${id}/installed`, { method: 'POST' });
        const data = await response.json();
        if (data.success) {
            showToast('Marked as installed!', 'success');
            loadDownloads();
        } else {
            showToast(data.message, 'error');
        }
    } catch (error) {
        showToast('Error updating status', 'error');
    }
}

function copyPath(path) {
    navigator.clipboard.writeText(path).then(() => {
        showToast('Path copied to clipboard', 'success');
    }).catch(() => {
        // Fallback
        prompt('Copy this path:', path);
    });
}

// ─── Upload CSV ───

function showUploadModal() {
    document.getElementById('uploadModal').classList.remove('hidden');
    document.getElementById('uploadError').classList.add('hidden');
}

function hideUploadModal() {
    document.getElementById('uploadModal').classList.add('hidden');
    document.getElementById('csvFile').value = '';
    document.getElementById('uploadError').classList.add('hidden');
}

async function uploadCSV() {
    const fileInput = document.getElementById('csvFile');
    const file = fileInput.files[0];
    const uploadBtn = document.getElementById('uploadBtn');
    const uploadError = document.getElementById('uploadError');

    uploadError.classList.add('hidden');

    if (!file) {
        uploadError.textContent = 'Please select a CSV file';
        uploadError.classList.remove('hidden');
        return;
    }

    const formData = new FormData();
    formData.append('file', file);

    uploadBtn.disabled = true;
    uploadBtn.textContent = 'Uploading...';

    try {
        const response = await fetch(`${API_BASE}/games/upload`, {
            method: 'POST',
            body: formData
        });

        const data = await response.json();

        if (data.success) {
            showToast(data.message, 'success');
            hideUploadModal();
            loadGames();
        } else {
            uploadError.textContent = data.message;
            uploadError.classList.remove('hidden');
        }
    } catch (error) {
        uploadError.textContent = 'Error uploading CSV';
        uploadError.classList.remove('hidden');
    } finally {
        uploadBtn.disabled = false;
        uploadBtn.textContent = 'Upload';
    }
}

// ─── Re-scrape ───

async function rescrape() {
    if (!confirm('This will scrape all games from the website. This may take several minutes. Continue?')) {
        return;
    }

    try {
        const response = await fetch(`${API_BASE}/games/rescrape`, { method: 'POST' });
        const data = await response.json();

        if (data.success) {
            showToast(data.message, 'info');
            startStatusPolling();
        } else {
            showToast(data.message, 'error');
        }
    } catch (error) {
        showToast('Error starting scrape', 'error');
    }
}

function startStatusPolling() {
    document.getElementById('scrapeProgressContainer').classList.remove('hidden');
    document.getElementById('scrapeBtn').disabled = true;
    statusCheckInterval = setInterval(checkScrapeStatus, 2000);
}

async function checkScrapeStatus() {
    try {
        const response = await fetch(`${API_BASE}/scrape-status`);
        const status = await response.json();

        if (status.is_running) {
            document.getElementById('scrapeProgressContainer').classList.remove('hidden');
            document.getElementById('scrapeBtn').disabled = true;

            // Update progress bar
            const pct = status.progress || 0;
            const bar = document.getElementById('scrapeProgressBar');
            bar.style.width = `${pct}%`;
            document.getElementById('scrapeProgressPct').textContent = `${pct.toFixed(1)}%`;

            // Color by phase
            bar.classList.remove('enriching', 'saving');
            if (status.phase === 'enriching') {
                bar.classList.add('enriching');
            } else if (status.phase === 'saving') {
                bar.classList.add('saving');
            }

            // Update message
            const msg = status.message || 'Scraping...';
            document.getElementById('scrapeProgressMessage').textContent = msg;

            // Update stats based on phase
            let statsText = '';
            if (status.phase === 'fetching_pages') {
                statsText = `${status.pages_found || 0} pages fetched`;
            } else if (status.phase === 'scraping_games') {
                statsText = `${status.games_scraped || 0} / ${status.games_total || '?'} posts`;
            } else if (status.phase === 'enriching') {
                statsText = `${status.games_scraped || 0} / ${status.games_total || '?'} lookups`;
            } else if (status.phase === 'saving' || status.phase === 'done') {
                statsText = 'Saving to database...';
            }
            document.getElementById('scrapeProgressStats').textContent = statsText;

            // Show metadata counters once we're past initial fetch
            const metaContainer = document.getElementById('scrapeMetadataStats');
            if (status.phase === 'scraping_games' || status.phase === 'enriching' || status.phase === 'saving' || status.phase === 'done') {
                metaContainer.classList.remove('hidden');
                document.getElementById('statGames').textContent = status.games_scraped || 0;
                document.getElementById('statImages').textContent = status.with_thumbnail || 0;
                document.getElementById('statGenres').textContent = status.with_genres || 0;
                document.getElementById('statCompanies').textContent = status.with_company || 0;
            } else {
                metaContainer.classList.add('hidden');
            }

        } else {
            document.getElementById('scrapeProgressContainer').classList.add('hidden');
            document.getElementById('scrapeBtn').disabled = false;

            if (statusCheckInterval) {
                clearInterval(statusCheckInterval);
                statusCheckInterval = null;
            }

            if (status.last_result) {
                showToast(status.last_result, status.last_result.includes('failed') ? 'error' : 'success');
                loadGames();
            }
        }
    } catch (error) {
        console.error('Error checking scrape status:', error);
    }
}

// ─── Helpers ───

function getStatusIcon(status) {
    switch (status) {
        case 'queued': return '⏳';
        case 'downloading': return '📥';
        case 'extracting': return '📦';
        case 'completed': return '✅';
        case 'installing': return '🔧';
        case 'installed': return '🎮';
        case 'failed': return '❌';
        default: return '❓';
    }
}

function getStatusColor(status) {
    switch (status) {
        case 'queued': return 'bg-yellow-800 text-yellow-200';
        case 'downloading': return 'bg-blue-800 text-blue-200';
        case 'extracting': return 'bg-purple-800 text-purple-200';
        case 'completed': return 'bg-green-800 text-green-200';
        case 'installing': return 'bg-purple-800 text-purple-200';
        case 'installed': return 'bg-green-900 text-green-300';
        case 'failed': return 'bg-red-800 text-red-200';
        default: return 'bg-gray-700 text-gray-300';
    }
}

function formatBytes(bytes) {
    if (bytes >= 1073741824) return (bytes / 1073741824).toFixed(1) + ' GB';
    if (bytes >= 1048576) return (bytes / 1048576).toFixed(1) + ' MB';
    if (bytes >= 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return bytes + ' B';
}

function formatDate(dateStr) {
    try {
        const d = new Date(dateStr);
        return d.toLocaleString();
    } catch {
        return dateStr;
    }
}

function showToast(message, type = 'info') {
    const toast = document.getElementById('toast');
    const toastMessage = document.getElementById('toastMessage');

    toast.classList.remove('toast-success', 'toast-error', 'toast-info');

    if (type === 'success') toast.classList.add('toast-success');
    else if (type === 'error') toast.classList.add('toast-error');
    else toast.classList.add('toast-info');

    toastMessage.textContent = message;
    toast.classList.remove('hidden');

    setTimeout(() => {
        toast.classList.add('hidden');
    }, 5000);
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// ─── Genres ───

async function loadGenres() {
    try {
        const response = await fetch(`${API_BASE}/games/genres`);
        const data = await response.json();
        const select = document.getElementById('genreSelect');

        // Keep the "All Genres" option
        select.innerHTML = '<option value="">All Genres</option>';

        // Add top genres (show count)
        for (const g of data.genres.slice(0, 50)) {
            const opt = document.createElement('option');
            opt.value = g.name;
            opt.textContent = `${g.name} (${g.count})`;
            select.appendChild(opt);
        }
    } catch (error) {
        console.error('Failed to load genres:', error);
    }
}

function filterByGenre(genre) {
    document.getElementById('genreSelect').value = genre;
    // If the genre isn't in the dropdown (rare), add it temporarily
    const select = document.getElementById('genreSelect');
    if (select.value !== genre) {
        const opt = document.createElement('option');
        opt.value = genre;
        opt.textContent = genre;
        select.appendChild(opt);
        select.value = genre;
    }
    currentPage = 1;
    loadGames();
}

// ─── Favorites ───

async function loadFavoriteIds() {
    try {
        const response = await fetch(`${API_BASE}/games/favorites`);
        const data = await response.json();
        favoriteIds = new Set(data.ids || []);
    } catch (error) {
        console.error('Failed to load favorites:', error);
    }
}

async function toggleFavorite(gameId) {
    const isFav = favoriteIds.has(gameId);

    try {
        await fetch(`${API_BASE}/games/favorites/${gameId}`, {
            method: isFav ? 'DELETE' : 'POST'
        });

        if (isFav) {
            favoriteIds.delete(gameId);
        } else {
            favoriteIds.add(gameId);
        }

        // Re-render current games to update star icons
        if (currentGames.length > 0) {
            renderGames(currentGames);
        }

        // If showing favorites view and we just unfavorited, reload
        if (showingFavorites && isFav) {
            loadFavoritesView();
        }
    } catch (error) {
        showToast('Failed to update favorite', 'error');
    }
}

function toggleFavoritesView() {
    showingFavorites = !showingFavorites;
    const btn = document.getElementById('favToggle');
    if (showingFavorites) {
        btn.classList.remove('bg-gray-700');
        btn.classList.add('bg-yellow-700');
        loadFavoritesView();
    } else {
        btn.classList.add('bg-gray-700');
        btn.classList.remove('bg-yellow-700');
        loadGames();
    }
}

async function loadFavoritesView() {
    try {
        const response = await fetch(`${API_BASE}/games/favorites`);
        const data = await response.json();
        const games = data.favorites || [];
        currentGames = games;
        renderGames(games);
        document.getElementById('statsText').textContent = `⭐ ${games.length} favorite${games.length !== 1 ? 's' : ''}`;
        document.getElementById('pagination').innerHTML = '';

        if (games.length === 0) {
            document.getElementById('emptyState').classList.remove('hidden');
        } else {
            document.getElementById('emptyState').classList.add('hidden');
        }
    } catch (error) {
        showToast('Failed to load favorites', 'error');
    }
}

// ─── Random Game ───

async function randomGame() {
    try {
        const response = await fetch(`${API_BASE}/games/random`);
        const data = await response.json();
        if (data.game) {
            currentGames = [data.game];
            showGameModal(data.game.id);
        }
    } catch (error) {
        showToast('Failed to get random game', 'error');
    }
}

// ─── Settings ───

async function showSettingsModal() {
    document.getElementById('settingsModal').classList.remove('hidden');

    // Clear inputs
    document.getElementById('settingRawgKey').value = '';
    document.getElementById('settingRdKey').value = '';
    document.getElementById('rawgKeyStatus').textContent = 'Loading...';
    document.getElementById('rdKeyStatus').textContent = 'Loading...';

    try {
        const response = await fetch(`${API_BASE}/settings`);
        const data = await response.json();
        const s = data.settings;

        if (s.rawg_api_key_set === 'true') {
            document.getElementById('rawgKeyStatus').innerHTML = `<span class="text-green-400">✓ Set</span> <span class="text-gray-500">(${s.rawg_api_key_masked})</span> — leave blank to keep current`;
        } else {
            document.getElementById('rawgKeyStatus').innerHTML = '<span class="text-yellow-400">⚠ Not set</span> — images won\'t load without this';
        }

        if (s.rd_api_key_set === 'true') {
            document.getElementById('rdKeyStatus').innerHTML = `<span class="text-green-400">✓ Set</span> <span class="text-gray-500">(${s.rd_api_key_masked})</span> — leave blank to keep current`;
        } else {
            document.getElementById('rdKeyStatus').innerHTML = '<span class="text-yellow-400">⚠ Not set</span> — downloads won\'t work without this';
        }
    } catch (error) {
        document.getElementById('rawgKeyStatus').textContent = 'Failed to load settings';
        document.getElementById('rdKeyStatus').textContent = 'Failed to load settings';
    }
}

function hideSettingsModal() {
    document.getElementById('settingsModal').classList.add('hidden');
}

function toggleKeyVisibility(inputId) {
    const input = document.getElementById(inputId);
    input.type = input.type === 'password' ? 'text' : 'password';
}

async function saveSettings() {
    const rawgKey = document.getElementById('settingRawgKey').value.trim();
    const rdKey = document.getElementById('settingRdKey').value.trim();

    const settings = {};
    // Only send keys that were actually entered (non-empty means update)
    if (rawgKey) settings.rawg_api_key = rawgKey;
    if (rdKey) settings.rd_api_key = rdKey;

    if (Object.keys(settings).length === 0) {
        showToast('No changes to save', 'info');
        hideSettingsModal();
        return;
    }

    try {
        const response = await fetch(`${API_BASE}/settings`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ settings })
        });

        const data = await response.json();
        if (data.success) {
            showToast('Settings saved!', 'success');
            hideSettingsModal();
        } else {
            showToast(`Error: ${data.message}`, 'error');
        }
    } catch (error) {
        showToast('Failed to save settings', 'error');
    }
}
