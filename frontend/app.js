const API_BASE = '/api';

// Override fetch to always include credentials (session cookie)
const _originalFetch = window.fetch;
window.fetch = function(url, options = {}) {
    options.credentials = options.credentials || 'include';
    return _originalFetch.call(this, url, options);
};

let currentPage = 1;
let totalPages = 1;
let isLoadingMore = false;
let selectedGameId = null;
let searchTimeout = null;
let statusCheckInterval = null;
let downloadPollInterval = null;
let currentView = 'games'; // 'games' or 'downloads'
let favoriteIds = new Set();
let showingFavorites = false;
let selectedSource = 'all';
let currentUser = null; // Stores current authenticated user
let notificationPollInterval = null;
let viewMode = localStorage.getItem('viewMode') || 'list'; // 'list' or 'card'

// ─── Authentication ───

async function checkAuth() {
    try {
        const response = await fetch(`${API_BASE}/auth/me`, {
            credentials: 'include'
        });
        const data = await response.json();

        if (data.success && data.user) {
            currentUser = data.user;
            updateUserUI();
            return true;
        } else {
            // Not authenticated, redirect to login
            window.location.href = '/login.html';
            return false;
        }
    } catch (error) {
        console.error('Auth check failed:', error);
        window.location.href = '/login.html';
        return false;
    }
}

function updateUserUI() {
    if (!currentUser) return;

    // Show user info
    document.getElementById('userName').textContent = currentUser.username;
    document.getElementById('userInfo').classList.remove('hidden');
    document.getElementById('logoutBtn').classList.remove('hidden');

    // Show admin badge if admin
    if (currentUser.is_admin) {
        document.getElementById('userAdmin').classList.remove('hidden');
    }
}

async function handleLogout() {
    try {
        const response = await fetch(`${API_BASE}/auth/logout`, {
            method: 'POST',
            credentials: 'include'
        });

        const data = await response.json();

        if (data.success) {
            window.location.href = '/login.html';
        }
    } catch (error) {
        console.error('Logout failed:', error);
        window.location.href = '/login.html';
    }
}

// Load games on page load
document.addEventListener('DOMContentLoaded', async () => {
    // Check authentication first
    const isAuthenticated = await checkAuth();
    if (!isAuthenticated) {
        return; // Will redirect to login
    }

    // Load app data
    loadGames();
    loadGenres();
    loadFavoriteIds();
    // Start notification polling
    startNotificationPolling();
    // Check if a scrape is already running (e.g. page refresh during scrape)
    checkScrapeStatus().then(() => {
        const container = document.getElementById('scrapeProgressContainer');
        if (container && !container.classList.contains('hidden')) {
            statusCheckInterval = setInterval(checkScrapeStatus, 2000);
        }
    });
});

// ─── Keyboard Shortcuts ───

document.addEventListener('keydown', (e) => {
    // Don't trigger shortcuts when typing in inputs
    const tag = document.activeElement.tagName;
    const isInput = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';

    // Escape: close any open modal
    if (e.key === 'Escape') {
        if (!document.getElementById('confirmModal').classList.contains('hidden')) {
            hideConfirmModal();
        } else if (!document.getElementById('settingsModal').classList.contains('hidden')) {
            hideSettingsModal();
        } else if (!document.getElementById('uploadModal').classList.contains('hidden')) {
            hideUploadModal();
        }
        return;
    }

    // Arrow keys: screenshot gallery navigation (when modal is open)
    if (!document.getElementById('confirmModal').classList.contains('hidden') && modalScreenshots.length > 1) {
        if (e.key === 'ArrowLeft') { e.preventDefault(); prevScreenshot(); return; }
        if (e.key === 'ArrowRight') { e.preventDefault(); nextScreenshot(); return; }
    }

    if (isInput) return;

    // / or Ctrl+K: focus search
    if (e.key === '/' || (e.key === 'k' && (e.metaKey || e.ctrlKey))) {
        e.preventDefault();
        document.getElementById('searchInput').focus();
        return;
    }

    // r: random game
    if (e.key === 'r') { randomGame(); return; }

    // f: toggle favorites
    if (e.key === 'f') { toggleFavoritesView(); return; }

    // 0: home, 1: games view, 2: downloads view, 3: system health
    if (e.key === '0') { showView('home'); return; }
    if (e.key === '1') { showView('games'); return; }
    if (e.key === '2') { showView('downloads'); return; }
    if (e.key === '3') { showView('systemHealth'); return; }
});

// ─── View switching ───

function showView(view) {
    currentView = view;
    document.getElementById('homeView').classList.toggle('hidden', view !== 'home');
    document.getElementById('gamesView').classList.toggle('hidden', view !== 'games');
    document.getElementById('libraryView').classList.toggle('hidden', view !== 'library');
    document.getElementById('downloadsView').classList.toggle('hidden', view !== 'downloads');
    document.getElementById('clientsView').classList.toggle('hidden', view !== 'clients');
    document.getElementById('systemHealthView').classList.toggle('hidden', view !== 'systemHealth');

    // Update sidebar navigation active states
    document.getElementById('sidebarHome').classList.toggle('active', view === 'home');
    document.getElementById('sidebarGames').classList.toggle('active', view === 'games');
    document.getElementById('sidebarLibrary').classList.toggle('active', view === 'library');
    document.getElementById('sidebarDownloads').classList.toggle('active', view === 'downloads');
    document.getElementById('sidebarClients').classList.toggle('active', view === 'clients');
    document.getElementById('sidebarSystemHealth').classList.toggle('active', view === 'systemHealth');

    if (view === 'home') {
        // Load featured games carousel if not already loaded
        if (featuredGames.length === 0) {
            loadFeaturedCategory('hot');
        }
    }

    if (view === 'library') {
        switchLibraryTab('favorites');
    }

    if (view === 'downloads') {
        loadDownloads();
        startDownloadPolling();
    } else {
        stopDownloadPolling();
    }

    if (view === 'systemHealth') {
        loadSystemHealth();
    }

    if (view === 'clients') {
        loadClients();
    }
}

// ─── Games ───

async function loadGames(page = 1, append = false) {
    if (isLoadingMore && append) return;

    currentPage = page;
    if (!append) {
        isLoadingMore = false;
    }

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
    if (selectedSource) params.append('source', selectedSource);

    if (!append) {
        showLoading(true);
    } else {
        isLoadingMore = true;
        showScrollLoader(true);
    }
    hideError();

    try {
        const response = await fetch(`${API_BASE}/games?${params}`);
        if (!response.ok) throw new Error('Failed to load games');

        const data = await response.json();
        totalPages = data.total_pages || 1;

        if (append) {
            currentGames = currentGames.concat(data.games || []);
            appendGames(data.games || []);
        } else {
            currentGames = data.games || [];
            renderGames(data.games);
        }

        updateStats(data);

        if (data.total === 0 && !append) {
            document.getElementById('emptyState').classList.remove('hidden');
        } else {
            document.getElementById('emptyState').classList.add('hidden');
        }

        // Refresh genre counts
        loadGenres();
    } catch (error) {
        showError('Failed to load games. Please try again.');
        console.error('Error loading games:', error);
    } finally {
        if (!append) {
            showLoading(false);
        } else {
            isLoadingMore = false;
            showScrollLoader(false);
        }
    }
}

function showLoading(show) {
    document.getElementById('loadingIndicator').classList.toggle('hidden', !show);
    document.getElementById('gamesGrid').classList.toggle('hidden', show);
}

function showScrollLoader(show) {
    let loader = document.getElementById('scrollLoader');
    if (!loader) {
        loader = document.createElement('div');
        loader.id = 'scrollLoader';
        loader.className = 'loading-state';
        loader.innerHTML = '<div style="text-align:center"><div class="spinner" style="width:28px;height:28px;border-width:3px;"></div></div>';
        document.getElementById('gamesGrid').parentElement.insertBefore(loader, document.getElementById('pagination'));
    }
    loader.classList.toggle('hidden', !show);
}

function appendGames(games) {
    const grid = document.getElementById('gamesGrid');
    if (games.length === 0) return;

    const fragment = document.createDocumentFragment();
    const temp = document.createElement('div');
    temp.innerHTML = games.map(game => buildCardHtml(game)).join('');
    while (temp.firstChild) {
        fragment.appendChild(temp.firstChild);
    }
    grid.appendChild(fragment);
}

// Infinite scroll observer
let scrollObserver = null;

function setupScrollObserver() {
    if (scrollObserver) scrollObserver.disconnect();

    const sentinel = document.getElementById('scrollSentinel');
    if (!sentinel) return;

    scrollObserver = new IntersectionObserver((entries) => {
        if (entries[0].isIntersecting && !isLoadingMore && !showingFavorites && currentPage < totalPages) {
            loadGames(currentPage + 1, true);
        }
    }, { rootMargin: '400px' });

    scrollObserver.observe(sentinel);
}

// Build a single card's HTML (shared by renderGames and appendGames)
function buildCardHtml(game) {
    const hasThumb = game.thumbnail_url && game.thumbnail_url.length > 0;
    const isFav = favoriteIds.has(game.id);
    const source = game.source || 'fitgirl';
    const sourceLabel = source === 'steamrip' ? 'SteamRIP' : 'FitGirl';
    const sourceBadge = `<span class="source-badge ${source}">${sourceLabel}</span>`;

    const thumb = hasThumb
        ? `<div class="game-thumb"><img src="${escapeHtml(game.thumbnail_url)}" alt="" loading="lazy" onerror="this.parentElement.classList.add('game-thumb-fallback');this.remove()">${sourceBadge}<button onclick="event.stopPropagation();toggleFavorite(${game.id})" class="fav-star" title="${isFav ? 'Remove from favorites' : 'Add to favorites'}">${isFav ? '⭐' : '☆'}</button></div>`
        : `<div class="game-thumb game-thumb-fallback"><span>🎮</span>${sourceBadge}<button onclick="event.stopPropagation();toggleFavorite(${game.id})" class="fav-star" title="${isFav ? 'Remove from favorites' : 'Add to favorites'}">${isFav ? '⭐' : '☆'}</button></div>`;

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
                <div class="card-info">
                    <div class="card-header">
                        <h3 class="card-title">${escapeHtml(game.title)}</h3>
                        ${yearBadge}
                    </div>
                    ${company}
                    ${genres}
                    <div class="card-size">${sizes}</div>
                </div>
                <div class="card-actions" style="display:none;">
                    <button onclick="event.stopPropagation();toggleFavorite(${game.id})" class="fav-star" title="${isFav ? 'Remove from favorites' : 'Add to favorites'}" style="position:static;margin:0;">${isFav ? '⭐' : '☆'}</button>
                </div>
            </div>
        </div>
    `;
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

    grid.innerHTML = games.map(game => buildCardHtml(game)).join('');

    // Setup infinite scroll after render
    requestAnimationFrame(() => setupScrollObserver());
}

function updateStats(data) {
    const loaded = currentGames.length;
    const total = data.total;
    const text = loaded < total
        ? `${loaded} of ${total} games loaded`
        : `${total} game${total !== 1 ? 's' : ''}`;
    document.getElementById('statsText').textContent = text;
}

function handleSearchChange() {
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
        currentPage = 1;
        currentGames = [];
        loadGames();
    }, 500);
}

function clearFilters() {
    document.getElementById('searchInput').value = '';
    document.getElementById('sortSelect').value = '';
    document.getElementById('genreSelect').value = '';
    setSource('all');
    showingFavorites = false;
    document.getElementById('favToggle').classList.remove('bg-yellow-700');
    currentPage = 1;
    currentGames = [];
    loadGames();
}

function setSource(source) {
    selectedSource = source;

    // Update active button states
    document.querySelectorAll('.source-toggle-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.source === source);
    });

    // Reload games with new source filter
    currentPage = 1;
    currentGames = [];
    loadGames();
}

function setViewMode(mode) {
    viewMode = mode;
    localStorage.setItem('viewMode', mode);

    // Update toggle button states
    document.getElementById('viewList').classList.toggle('active', mode === 'list');
    document.getElementById('viewCard').classList.toggle('active', mode === 'card');

    // Update grid container class
    const gamesView = document.getElementById('gamesView');
    gamesView.classList.toggle('view-mode-list', mode === 'list');
    gamesView.classList.toggle('view-mode-card', mode === 'card');
}

// Apply saved view mode on load
window.addEventListener('DOMContentLoaded', () => {
    setViewMode(viewMode);
});

// ─── Library View ───

function switchLibraryTab(tab) {
    // Update tab active states
    document.getElementById('tabFavorites').classList.toggle('active', tab === 'favorites');
    document.getElementById('tabDownloaded').classList.toggle('active', tab === 'downloaded');

    // Show/hide tab content
    document.getElementById('libraryFavoritesTab').classList.toggle('hidden', tab !== 'favorites');
    document.getElementById('libraryDownloadedTab').classList.toggle('hidden', tab !== 'downloaded');

    // Load content
    if (tab === 'favorites') {
        loadLibraryFavorites();
    } else if (tab === 'downloaded') {
        loadLibraryDownloaded();
    }
}

async function loadLibraryFavorites() {
    const grid = document.getElementById('favoritesGrid');
    const empty = document.getElementById('favoritesEmpty');

    try {
        const response = await fetch(`${API_BASE}/favorites`);
        if (!response.ok) throw new Error('Failed to load favorites');

        const data = await response.json();
        const favoriteGames = data.favorites || [];

        if (favoriteGames.length === 0) {
            grid.innerHTML = '';
            empty.classList.remove('hidden');
            return;
        }

        empty.classList.add('hidden');
        grid.innerHTML = favoriteGames.map(game => buildCardHtml(game)).join('');
    } catch (error) {
        console.error('Error loading favorites:', error);
        grid.innerHTML = '<p style="text-align:center;color:var(--text-dim);padding:2rem;">Failed to load favorites.</p>';
    }
}

async function loadLibraryDownloaded() {
    const grid = document.getElementById('downloadedGrid');
    const empty = document.getElementById('downloadedEmpty');

    try {
        // Fetch completed downloads
        const downloadsRes = await fetch(`${API_BASE}/downloads`);
        if (!downloadsRes.ok) throw new Error('Failed to load downloads');

        const downloadsData = await downloadsRes.json();
        const completedDownloads = (downloadsData.downloads || [])
            .filter(d => d.status === 'completed')
            .map(d => d.game_id);

        if (completedDownloads.length === 0) {
            grid.innerHTML = '';
            empty.classList.remove('hidden');
            return;
        }

        // Fetch games by IDs
        const gamesRes = await fetch(`${API_BASE}/games?ids=${completedDownloads.join(',')}`);
        if (!gamesRes.ok) throw new Error('Failed to load games');

        const gamesData = await gamesRes.json();
        const games = gamesData.games || [];

        if (games.length === 0) {
            grid.innerHTML = '';
            empty.classList.remove('hidden');
            return;
        }

        empty.classList.add('hidden');
        grid.innerHTML = games.map(game => buildCardHtml(game)).join('');
    } catch (error) {
        console.error('Error loading downloaded games:', error);
        grid.innerHTML = '<p style="text-align:center;color:var(--text-dim);padding:2rem;">Failed to load downloaded games.</p>';
    }
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
        <div style="position:relative;margin-bottom:0.75rem;background:var(--bg-deep);border-radius:10px;overflow:hidden">
            <img id="modalScreenshot" src="${escapeHtml(modalScreenshots[0])}" alt="" style="width:100%;max-height:16rem;object-fit:contain;display:block" onerror="this.style.display='none'">
            ${modalScreenshots.length > 1 ? `
                <button onclick="prevScreenshot()" style="position:absolute;left:0.5rem;top:50%;transform:translateY(-50%);background:rgba(0,0,0,0.6);color:#fff;border:none;border-radius:50%;width:2rem;height:2rem;cursor:pointer;font-size:1rem;display:flex;align-items:center;justify-content:center">‹</button>
                <button onclick="nextScreenshot()" style="position:absolute;right:0.5rem;top:50%;transform:translateY(-50%);background:rgba(0,0,0,0.6);color:#fff;border:none;border-radius:50%;width:2rem;height:2rem;cursor:pointer;font-size:1rem;display:flex;align-items:center;justify-content:center">›</button>
                <span id="screenshotCounter" style="position:absolute;bottom:0.5rem;left:50%;transform:translateX(-50%);background:rgba(0,0,0,0.6);color:#fff;font-size:0.7rem;padding:0.15rem 0.5rem;border-radius:999px;font-family:'JetBrains Mono',monospace">1 / ${modalScreenshots.length}</span>
            ` : ''}
        </div>
        ${modalScreenshots.length > 1 ? `
            <div style="display:flex;gap:0.3rem;margin-bottom:0.75rem;overflow-x:auto;padding-bottom:0.25rem">
                ${modalScreenshots.map((url, i) => `
                    <img src="${escapeHtml(url)}" alt="" onclick="goToScreenshot(${i})"
                         class="screenshot-thumb ${i === 0 ? 'border-blue-500' : 'border-transparent'}"
                         style="height:3rem;width:5rem;object-fit:cover;border-radius:6px;cursor:pointer;flex-shrink:0"
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
    const sourceName = game.source === 'steamrip' ? 'SteamRIP' : 'FitGirl Repacks';
    const sourceLink = game.source_url
        ? `<p style="margin-bottom:0.25rem"><a href="${encodeURI(game.source_url)}" target="_blank" style="color:var(--accent-bright);font-size:0.85rem;text-decoration:none">View on ${escapeHtml(sourceName)} →</a></p>`
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
        <button id="downloadBtn" onclick="queueDownload(${gameId})"
                class="btn btn-primary" style="flex:1">
            Download
        </button>
        <button id="rdLinksBtn" onclick="addToRealDebrid(${gameId})"
                class="btn btn-ghost" style="flex:1">
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
        downloadBtn.innerHTML = '<span class="spinner"></span> Checking client...';
    }

    try {
        // Step 1: Check if user has a connected client
        const statusResponse = await fetch(`${API_BASE}/clients/status`);
        const clientStatus = await statusResponse.json();

        if (!clientStatus.has_client) {
            showToast('❌ No client registered. Please install and run the Windows client on your PC.', 'error');
            if (downloadBtn) {
                downloadBtn.disabled = false;
                downloadBtn.innerHTML = 'Download';
            }
            return;
        }

        if (!clientStatus.client_online) {
            showToast('❌ Client is offline. Please start the Windows client on your PC.', 'error');
            if (downloadBtn) {
                downloadBtn.disabled = false;
                downloadBtn.innerHTML = 'Download';
            }
            return;
        }

        // Step 2: Client is online, create download on server
        if (downloadBtn) {
            downloadBtn.innerHTML = '<span class="spinner"></span> Creating download...';
        }

        const response = await fetch(`${API_BASE}/downloads/create`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ game_id: gameId })
        });

        const data = await response.json();

        if (data.success) {
            showToast('✅ Download queued! Your client will start downloading shortly.', 'success');
            hideConfirmModal();
            // Optionally refresh downloads view
            if (typeof loadDownloads === 'function') {
                setTimeout(() => loadDownloads(), 1000);
            }
        } else {
            showToast(data.message || 'Failed to queue download', 'error');
            if (downloadBtn) {
                downloadBtn.disabled = false;
                downloadBtn.innerHTML = 'Download';
            }
        }
    } catch (error) {
        showToast('Failed to queue download. Please try again.', 'error');
        console.error('Queue download error:', error);
        if (downloadBtn) {
            downloadBtn.disabled = false;
            downloadBtn.innerHTML = 'Download';
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
        <p style="font-weight:700;color:var(--green);margin-bottom:0.75rem;font-size:0.95rem">${escapeHtml(message)}</p>
        <div style="display:flex;flex-direction:column;gap:0.5rem;max-height:24rem;overflow-y:auto">
            ${downloads.map(dl => `
                <a href="${escapeHtml(dl.download_url)}"
                   download
                   target="_blank"
                   style="display:block;background:var(--bg-surface);padding:0.75rem;border-radius:10px;border:1px solid var(--border);text-decoration:none;color:var(--text);transition:border-color 0.15s"
                   onmouseover="this.style.borderColor='var(--green)'" onmouseout="this.style.borderColor='var(--border)'">
                    <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.15rem">${escapeHtml(dl.filename)}</div>
                    <div style="font-size:0.7rem;color:var(--text-dim)">Click to download</div>
                </a>
            `).join('')}
        </div>
        <p style="font-size:0.7rem;color:var(--text-dim);margin-top:0.75rem">Direct download links from Real-Debrid. Links expire after a limited time.</p>
    `;

    document.getElementById('confirmBtnContainer').innerHTML = `
        <button onclick="copyAllLinks()" class="btn btn-ghost" style="flex:0">
            Copy All Links
        </button>
        <button onclick="hideConfirmModal()" class="btn btn-secondary">
            Close
        </button>
    `;

    // Stash links for copy-all
    window._lastDownloadLinks = downloads.map(dl => dl.download_url);

    showToast('Download links ready!', 'success');
}

function copyAllLinks() {
    const links = window._lastDownloadLinks || [];
    if (links.length === 0) return;
    navigator.clipboard.writeText(links.join('\n')).then(() => {
        showToast(`Copied ${links.length} link${links.length > 1 ? 's' : ''} to clipboard`, 'success');
    }).catch(() => {
        prompt('Copy these links:', links.join('\n'));
    });
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

        // Update badge count (both topbar and sidebar)
        const activeCount = data.downloads.filter(d =>
            d.status === 'queued' || d.status === 'downloading' || d.status === 'extracting'
        ).length;

        const sidebarBadge = document.getElementById('sidebarDownloadBadge');
        if (activeCount > 0) {
            sidebarBadge.textContent = activeCount;
            sidebarBadge.classList.remove('hidden');
        } else {
            sidebarBadge.classList.add('hidden');
        }
    } catch (error) {
        console.error('Error loading downloads:', error);
    }
}

function renderDownloads(downloads) {
    const container = document.getElementById('downloadsList');

    if (downloads.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <p>No downloads yet</p>
                <p>Click a game and hit "Get Links" to get started.</p>
            </div>
        `;
        return;
    }

    container.innerHTML = downloads.map(dl => {
        const statusLabel = getStatusLabel(dl.status);
        const statusStyle = getStatusStyle(dl.status);
        const progressPct = Math.min(100, Math.max(0, dl.progress || 0));

        let statsHtml = '';
        let actionsHtml = '';
        let extractPct = null;
        let extractInfo = null;

        switch (dl.status) {
            case 'queued':
                statsHtml = '<span style="color:var(--gold)">Waiting in queue...</span>';
                actionsHtml = `<button onclick="cancelDownload(${dl.id})" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem;color:var(--red)">Cancel</button>`;
                break;

            case 'downloading':
                const speedStr = dl.download_speed || '—';
                const etaStr = dl.eta || '—';
                const progressStr = `${progressPct.toFixed(1)}%`;
                statsHtml = `
                    <span>${progressStr}</span>
                    <span style="margin:0 0.35rem;color:var(--text-dim)">·</span>
                    <span>${speedStr}</span>
                    <span style="margin:0 0.35rem;color:var(--text-dim)">·</span>
                    <span>${etaStr} remaining</span>
                `;
                actionsHtml = `<button onclick="cancelDownload(${dl.id})" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem;color:var(--red)">Cancel</button>`;
                break;

            case 'extracting':
                if (dl.extract_progress) {
                    const ep = dl.extract_progress;
                    const epPct = Math.min(100, Math.max(0, ep.percent || 0));
                    const filesInfo = ep.files_total > 0
                        ? `${ep.files_done}/${ep.files_total} files`
                        : `${ep.files_done} files`;
                    statsHtml = `<span style="color:var(--purple)">${escapeHtml(ep.message)}</span>`;
                    extractPct = epPct;
                    extractInfo = filesInfo;
                } else {
                    statsHtml = '<span style="color:var(--purple)">Extracting archives...</span>';
                }
                break;

            case 'completed':
                statsHtml = `<span style="color:var(--green)">Ready to install${dl.completed_at ? ' · ' + formatDate(dl.completed_at) : ''}</span>`;
                const hasMultipleFiles = dl.files && dl.files.length > 1;
                const md5ButtonDisabled = !dl.has_md5;
                actionsHtml = `
                    ${dl.installer_path ? `<button onclick="launchInstall(${dl.id})" class="btn btn-primary" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem">Install</button>` : ''}
                    <button
                        onclick="${md5ButtonDisabled ? 'showToast(\'No MD5 file found in download\', \'error\')' : `validateMD5(${dl.id})`}"
                        class="btn btn-ghost"
                        style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem;${md5ButtonDisabled ? 'opacity:0.4;cursor:not-allowed' : ''}"
                        ${md5ButtonDisabled ? 'disabled' : ''}>
                        ✓ Validate MD5
                    </button>
                    ${hasMultipleFiles ? `<button onclick="downloadAllFiles(${dl.id})" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem">⬇ Download All</button>` : ''}
                    ${dl.file_path ? `<button onclick="copyPath('${escapeHtml(dl.file_path)}')" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem">Copy Path</button>` : ''}
                    <button onclick="deleteDownload(${dl.id}, '${escapeHtml(dl.game_title).replace(/'/g, "\\'")}')" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem;color:var(--red)">🗑 Delete Files</button>
                `;
                break;

            case 'installing':
                statsHtml = '<span style="color:var(--purple)">Installer launched — complete the setup wizard</span>';
                actionsHtml = `
                    <button onclick="markInstalled(${dl.id})" class="btn btn-primary" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem">Done Installing</button>
                    <button onclick="launchInstall(${dl.id})" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem">Relaunch</button>
                `;
                break;

            case 'installed':
                statsHtml = `<span style="color:var(--green)">Installed${dl.completed_at ? ' · ' + formatDate(dl.completed_at) : ''}</span>`;
                const hasMultipleFilesInstalled = dl.files && dl.files.length > 1;
                const md5ButtonDisabledInstalled = !dl.has_md5;
                actionsHtml = `
                    <button
                        onclick="${md5ButtonDisabledInstalled ? 'showToast(\'No MD5 file found in download\', \'error\')' : `validateMD5(${dl.id})`}"
                        class="btn btn-ghost"
                        style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem;${md5ButtonDisabledInstalled ? 'opacity:0.4;cursor:not-allowed' : ''}"
                        ${md5ButtonDisabledInstalled ? 'disabled' : ''}>
                        ✓ Validate MD5
                    </button>
                    ${hasMultipleFilesInstalled ? `<button onclick="downloadAllFiles(${dl.id})" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem">⬇ Download All</button>` : ''}
                    ${dl.file_path ? `<button onclick="copyPath('${escapeHtml(dl.file_path)}')" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem">Copy Path</button>` : ''}
                    <button onclick="deleteDownload(${dl.id}, '${escapeHtml(dl.game_title).replace(/'/g, "\\'")}')" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem;color:var(--red)">🗑 Delete Files</button>
                `;
                break;

            case 'failed':
                statsHtml = `<span style="color:var(--red)">${escapeHtml(dl.error_message || 'Unknown error')}</span>`;
                actionsHtml = `
                    <button onclick="retryDownload(${dl.id})" class="btn btn-gold" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem">Retry</button>
                    <button onclick="removeDownload(${dl.id})" class="btn btn-ghost" style="flex:0;padding:0.35rem 0.75rem;font-size:0.75rem">Remove</button>
                `;
                break;
        }

        const showProgress = dl.status === 'downloading' || dl.status === 'extracting';
        const barPct = dl.status === 'extracting' && extractPct !== null ? extractPct : progressPct;
        const barClass = dl.status === 'extracting' ? 'enriching' : '';

        return `
            <div style="background:var(--bg-card);border:1px solid var(--border);border-radius:12px;padding:1rem;margin-bottom:0.625rem">
                <div style="display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:0.5rem">
                    <div>
                        <h3 style="font-weight:700;font-size:0.95rem;margin-bottom:0.15rem">${escapeHtml(dl.game_title)}</h3>
                        <p style="font-size:0.775rem;color:var(--text-dim);font-family:'JetBrains Mono',monospace">${escapeHtml(dl.game_size)}</p>
                    </div>
                    <span style="${statusStyle};font-size:0.675rem;font-weight:600;padding:0.2rem 0.6rem;border-radius:999px">${statusLabel}</span>
                </div>
                ${showProgress ? `
                    <div class="progress-track" style="margin-bottom:0.35rem">
                        <div class="progress-fill ${barClass}" style="width:${barPct}%"></div>
                    </div>
                    ${dl.status === 'extracting' && extractPct !== null ? `
                        <div style="display:flex;justify-content:space-between;font-size:0.675rem;color:var(--text-dim);margin-bottom:0.5rem;font-family:'JetBrains Mono',monospace">
                            <span>${extractInfo || ''}</span>
                            <span>${extractPct.toFixed(1)}%</span>
                        </div>
                    ` : ''}
                ` : ''}
                <div style="display:flex;justify-content:space-between;align-items:center;font-size:0.8rem;color:var(--text-muted)">
                    <div>${statsHtml}</div>
                    <div style="display:flex;gap:0.375rem">${actionsHtml}</div>
                </div>
                ${dl.files && dl.files.length > 0 ? `
                    <details style="margin-top:0.5rem">
                        <summary style="font-size:0.7rem;color:var(--text-dim);cursor:pointer">${dl.files.length} file(s)</summary>
                        <div style="margin-top:0.35rem;display:flex;flex-direction:column;gap:0.2rem">
                            ${dl.files.map(f => `
                                <div style="display:flex;justify-content:space-between;align-items:center;font-size:0.7rem;color:var(--text-dim);font-family:'JetBrains Mono',monospace">
                                    <span style="flex:1;overflow:hidden;text-overflow:ellipsis">${escapeHtml(f.filename)}</span>
                                    <div style="display:flex;align-items:center;gap:0.5rem">
                                        <span>${f.file_size ? formatBytes(f.file_size) : '—'}${f.is_extracted ? ' ✓' : ''}</span>
                                        ${f.file_path && (dl.status === 'completed' || dl.status === 'installed') ? `
                                            <button onclick="downloadFile(${f.id}, '${escapeHtml(f.filename)}')" class="btn btn-ghost" style="padding:0.15rem 0.4rem;font-size:0.65rem;min-width:auto" title="Download to your computer">
                                                <span style="font-size:0.9rem">⬇</span>
                                            </button>
                                        ` : ''}
                                    </div>
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
            showToast('Download removed from list', 'info');
            loadDownloads();
        } else {
            showToast(data.message, 'error');
        }
    } catch (error) {
        showToast('Error removing download', 'error');
    }
}

async function deleteDownload(id, gameName) {
    if (!confirm(`⚠️ Permanently delete "${gameName}" and all its files from disk?\n\nThis cannot be undone!`)) {
        return;
    }

    try {
        const response = await fetch(`${API_BASE}/downloads/${id}/delete`, { method: 'DELETE' });
        const data = await response.json();
        if (data.success) {
            showToast('Download and files deleted permanently', 'success');
            loadDownloads();
        } else {
            showToast(data.message, 'error');
        }
    } catch (error) {
        showToast('Error deleting download', 'error');
        console.error('Delete error:', error);
    }
}

async function scanExistingGames() {
    try {
        showToast('Scanning for existing games...', 'info');
        const response = await fetch(`${API_BASE}/downloads/scan`, { method: 'POST' });
        const data = await response.json();
        if (data.success) {
            showToast(data.message, 'success');
            loadDownloads();
        } else {
            showToast(data.message, 'error');
        }
    } catch (error) {
        showToast('Error scanning games', 'error');
        console.error('Scan error:', error);
    }
}

async function downloadFile(fileId, filename) {
    try {
        showToast(`Downloading ${filename}...`, 'info');

        // Create a link and trigger download
        const downloadUrl = `${API_BASE}/downloads/files/${fileId}`;
        const a = document.createElement('a');
        a.href = downloadUrl;
        a.download = filename;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);

        // Note: We can't easily show completion since browser handles the download
        setTimeout(() => {
            showToast('Download started in your browser', 'success');
        }, 500);
    } catch (error) {
        showToast('Error downloading file', 'error');
        console.error('Download error:', error);
    }
}

async function downloadAllFiles(downloadId) {
    try {
        // Get the download info to access files
        const response = await fetch(`${API_BASE}/downloads/${downloadId}`);
        if (!response.ok) {
            throw new Error('Failed to get download info');
        }

        const downloadInfo = await response.json();
        const files = downloadInfo.files || [];

        if (files.length === 0) {
            showToast('No files available to download', 'warning');
            return;
        }

        showToast(`Downloading ${files.length} file(s)...`, 'info');

        // Download each file with a small delay to avoid overwhelming the browser
        for (let i = 0; i < files.length; i++) {
            const file = files[i];
            if (file.file_path) {
                const downloadUrl = `${API_BASE}/downloads/files/${file.id}`;
                const a = document.createElement('a');
                a.href = downloadUrl;
                a.download = file.filename;
                document.body.appendChild(a);
                a.click();
                document.body.removeChild(a);

                // Small delay between downloads to avoid browser blocking
                if (i < files.length - 1) {
                    await new Promise(resolve => setTimeout(resolve, 300));
                }
            }
        }

        setTimeout(() => {
            showToast(`Started downloading ${files.length} file(s)`, 'success');
        }, 500);
    } catch (error) {
        showToast('Error downloading files', 'error');
        console.error('Download all error:', error);
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

async function validateMD5(id) {
    try {
        showToast('Validating MD5 checksums...', 'info');
        const response = await fetch(`${API_BASE}/downloads/${id}/validate`, { method: 'POST' });

        if (!response.ok) {
            const errorText = await response.text();
            showToast(`Validation failed: ${errorText}`, 'error');
            return;
        }

        const result = await response.json();

        // Show results in a modal
        let resultsHtml = `
            <div style="margin-bottom:1rem">
                <h3 style="margin-bottom:0.5rem;color:${result.failed > 0 ? 'var(--red)' : 'var(--green)'}">
                    ${result.status}
                </h3>
                <div style="font-size:0.875rem;color:var(--text-secondary)">
                    Total: ${result.total_files} | Valid: ${result.validated} | Failed: ${result.failed} | Skipped: ${result.skipped}
                </div>
            </div>
        `;

        if (result.files && result.files.length > 0) {
            resultsHtml += '<div style="max-height:400px;overflow-y:auto">';
            for (const file of result.files) {
                const statusColor = file.status === 'valid' ? 'var(--green)' :
                                  file.status === 'invalid' ? 'var(--red)' :
                                  file.status === 'missing' ? 'var(--orange)' :
                                  'var(--text-secondary)';
                const statusIcon = file.status === 'valid' ? '✓' :
                                 file.status === 'invalid' ? '✗' :
                                 file.status === 'missing' ? '?' : '–';

                resultsHtml += `
                    <div style="padding:0.5rem;margin-bottom:0.5rem;background:var(--surface);border-radius:4px;border-left:3px solid ${statusColor}">
                        <div style="display:flex;align-items:center;gap:0.5rem;margin-bottom:0.25rem">
                            <span style="color:${statusColor};font-weight:bold">${statusIcon}</span>
                            <span style="font-size:0.875rem;word-break:break-all">${escapeHtml(file.filename)}</span>
                        </div>
                        ${file.status === 'invalid' && file.expected_hash ? `
                            <div style="font-size:0.75rem;color:var(--text-secondary);margin-left:1.5rem">
                                Expected: ${file.expected_hash}<br>
                                Got: ${file.actual_hash || 'N/A'}
                            </div>
                        ` : ''}
                    </div>
                `;
            }
            resultsHtml += '</div>';
        }

        // Show modal with results using proper CSS classes
        const modal = document.createElement('div');
        modal.className = 'modal-overlay';
        modal.innerHTML = `
            <div class="modal-box" style="max-width:600px">
                <h2>MD5 Validation Results</h2>
                ${resultsHtml}
                <div style="display:flex;gap:0.5rem;margin-top:1rem">
                    <button onclick="this.closest('.modal-overlay').remove()" class="btn btn-primary" style="flex:1">Close</button>
                </div>
            </div>
        `;
        modal.addEventListener('click', (e) => {
            if (e.target === modal) modal.remove();
        });
        document.body.appendChild(modal);

        if (result.failed === 0) {
            showToast(`All ${result.validated} files validated successfully!`, 'success');
        } else {
            showToast(`${result.failed} file(s) failed validation`, 'error');
        }
    } catch (error) {
        console.error('Validation error:', error);
        showToast('Error validating MD5 checksums', 'error');
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

function getStatusLabel(status) {
    switch (status) {
        case 'queued': return 'QUEUED';
        case 'downloading': return 'DOWNLOADING';
        case 'extracting': return 'EXTRACTING';
        case 'completed': return 'COMPLETE';
        case 'installing': return 'INSTALLING';
        case 'installed': return 'INSTALLED';
        case 'failed': return 'FAILED';
        default: return status.toUpperCase();
    }
}

function getStatusStyle(status) {
    switch (status) {
        case 'queued': return 'background:var(--yellow-bg);color:var(--gold)';
        case 'downloading': return 'background:var(--accent-glow);color:var(--accent-bright)';
        case 'extracting': return 'background:var(--purple-dim);color:#a78bfa';
        case 'completed': return 'background:var(--green-dim);color:var(--green)';
        case 'installing': return 'background:var(--purple-dim);color:#a78bfa';
        case 'installed': return 'background:var(--green-dim);color:var(--green)';
        case 'failed': return 'background:var(--red-dim);color:var(--red)';
        default: return 'background:var(--bg-surface);color:var(--text-dim)';
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
    currentGames = [];
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
        document.getElementById('statsText').textContent = `${games.length} favorite${games.length !== 1 ? 's' : ''}`;
        document.getElementById('pagination').innerHTML = '';

        const emptyState = document.getElementById('emptyState');
        if (games.length === 0) {
            // Show enhanced favorites empty state
            emptyState.innerHTML = `
                <div class="empty-icon">⭐</div>
                <h3 class="empty-title">No favorites yet</h3>
                <p class="empty-subtitle">You haven't added any games to your favorites. Browse the library and click the star icon on games you want to save.</p>
                <div class="empty-action">
                    <button onclick="showingFavorites=false;document.getElementById('favToggle').classList.remove('bg-yellow-700');showView('games')" class="btn btn-primary">Browse All Games</button>
                </div>
            `;
            emptyState.classList.remove('hidden');
        } else {
            // Restore default empty state HTML for when filters are applied
            emptyState.innerHTML = `
                <div class="empty-icon">🎮</div>
                <h3 class="empty-title">No games found</h3>
                <p class="empty-subtitle">We couldn't find any games matching your filters. Try adjusting your search or clear all filters to browse the full library.</p>
                <div class="empty-action">
                    <button onclick="clearFilters()" class="btn btn-primary">Clear Filters</button>
                </div>
            `;
            emptyState.classList.add('hidden');
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

function switchSettingsTab(tabName) {
    // Update active tab
    document.querySelectorAll('.settings-tab').forEach(tab => {
        tab.classList.toggle('active', tab.dataset.tab === tabName);
    });

    // Show/hide panels
    document.querySelectorAll('.settings-panel').forEach(panel => {
        panel.classList.add('hidden');
    });
    document.getElementById(`settingsTab${tabName.charAt(0).toUpperCase() + tabName.slice(1)}`).classList.remove('hidden');
}

async function showSettingsModal() {
    document.getElementById('settingsModal').classList.remove('hidden');

    // Clear inputs
    document.getElementById('settingRawgKey').value = '';
    document.getElementById('settingRdKey').value = '';
    document.getElementById('rawgKeyStatus').textContent = 'Loading...';
    document.getElementById('rdKeyStatus').textContent = 'Loading...';

    // Reset to first tab
    switchSettingsTab('general');

    try {
        const response = await fetch(`${API_BASE}/settings`);
        const data = await response.json();
        const s = data.settings;

        // API Keys
        if (s.rawg_api_key_set === 'true') {
            document.getElementById('rawgKeyStatus').innerHTML = `<span style="color:var(--green)">✓ Set</span> <span style="color:var(--text-dim)">(${s.rawg_api_key_masked})</span> — leave blank to keep current`;
        } else {
            document.getElementById('rawgKeyStatus').innerHTML = '<span style="color:var(--gold)">Not set</span> — images won\'t load without this';
        }

        if (s.rd_api_key_set === 'true') {
            document.getElementById('rdKeyStatus').innerHTML = `<span style="color:var(--green)">✓ Set</span> <span style="color:var(--text-dim)">(${s.rd_api_key_masked})</span> — leave blank to keep current`;
        } else {
            document.getElementById('rdKeyStatus').innerHTML = '<span style="color:var(--gold)">Not set</span> — downloads won\'t work without this';
        }

        // User settings
        document.getElementById('settingDownloadPath').value = s.download_path || '';
        document.getElementById('settingTheme').value = s.theme || 'dark';
        document.getElementById('settingScraperFitgirl').checked = s.scraper_fitgirl_enabled !== 'false';
        document.getElementById('settingScraperSteamrip').checked = s.scraper_steamrip_enabled !== 'false';
        document.getElementById('settingNotifyDownloadComplete').checked = s.notify_download_complete !== 'false';
        document.getElementById('settingNotifyNewGames').checked = s.notify_new_games === 'true';
        document.getElementById('settingNotifyErrors').checked = s.notify_errors !== 'false';

        // Check notification permission status
        updateNotificationStatus();
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
    const settings = {};

    // API Keys (only if entered)
    const rawgKey = document.getElementById('settingRawgKey').value.trim();
    const rdKey = document.getElementById('settingRdKey').value.trim();
    if (rawgKey) settings.rawg_api_key = rawgKey;
    if (rdKey) settings.rd_api_key = rdKey;

    // User settings
    settings.download_path = document.getElementById('settingDownloadPath').value.trim();
    settings.theme = document.getElementById('settingTheme').value;
    settings.scraper_fitgirl_enabled = document.getElementById('settingScraperFitgirl').checked.toString();
    settings.scraper_steamrip_enabled = document.getElementById('settingScraperSteamrip').checked.toString();
    settings.notify_download_complete = document.getElementById('settingNotifyDownloadComplete').checked.toString();
    settings.notify_new_games = document.getElementById('settingNotifyNewGames').checked.toString();
    settings.notify_errors = document.getElementById('settingNotifyErrors').checked.toString();

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

async function requestNotificationPermission() {
    if (!('Notification' in window)) {
        showToast('Browser does not support notifications', 'error');
        return;
    }

    try {
        const permission = await Notification.requestPermission();
        updateNotificationStatus();

        if (permission === 'granted') {
            showToast('Browser notifications enabled!', 'success');
            // Show a test notification
            new Notification('Repack Browser', {
                body: 'Notifications are now enabled!',
                icon: '/favicon.ico'
            });
        } else if (permission === 'denied') {
            showToast('Notifications blocked. Please enable in browser settings.', 'error');
        }
    } catch (error) {
        showToast('Failed to request notification permission', 'error');
    }
}

function updateNotificationStatus() {
    const statusEl = document.getElementById('notificationStatus');
    const btnEl = document.getElementById('requestNotificationBtn');

    if (!('Notification' in window)) {
        statusEl.innerHTML = '<span style="color:var(--red);">Browser does not support notifications</span>';
        btnEl.disabled = true;
        return;
    }

    const permission = Notification.permission;
    if (permission === 'granted') {
        statusEl.innerHTML = '<span style="color:var(--green);">✓ Notifications enabled</span>';
        btnEl.textContent = 'Test Notification';
        btnEl.onclick = () => {
            new Notification('Repack Browser', {
                body: 'Test notification',
                icon: '/favicon.ico'
            });
        };
    } else if (permission === 'denied') {
        statusEl.innerHTML = '<span style="color:var(--red);">Notifications blocked. Enable in browser settings.</span>';
        btnEl.disabled = true;
    } else {
        statusEl.innerHTML = '<span style="color:var(--text-dim);">Click button to enable</span>';
        btnEl.textContent = 'Enable Browser Notifications';
    }
}

// ─── Client Management ───

async function loadClients() {
    const linkedContainer = document.getElementById('linkedClientsList');
    const unlinkedContainer = document.getElementById('unlinkedClientsList');

    try {
        const response = await fetch(`${API_BASE}/clients/mine`, {
            credentials: 'include'
        });
        const data = await response.json();

        if (data.linked && data.unlinked) {
            // Render linked clients
            if (data.linked.length === 0) {
                linkedContainer.innerHTML = `
                    <div class="empty-state">
                        <p>No linked clients</p>
                        <p>Link a client below to start receiving downloads on your PC.</p>
                    </div>
                `;
            } else {
                linkedContainer.innerHTML = data.linked.map(client => renderClientCard(client, true)).join('');
            }

            // Render unlinked clients
            if (data.unlinked.length === 0) {
                unlinkedContainer.innerHTML = `
                    <div class="empty-state">
                        <p>No available clients</p>
                        <p>Make sure the Repack Client is running on your PC and has connected to the server at least once.</p>
                    </div>
                `;
            } else {
                unlinkedContainer.innerHTML = data.unlinked.map(client => renderClientCard(client, false)).join('');
            }
        } else {
            throw new Error('Invalid response from server');
        }
    } catch (error) {
        console.error('Failed to load clients:', error);
        linkedContainer.innerHTML = `<div class="empty-state"><p style="color:var(--red);">Failed to load clients</p></div>`;
        unlinkedContainer.innerHTML = `<div class="empty-state"><p style="color:var(--red);">Failed to load clients</p></div>`;
    }
}

function renderClientCard(client, isLinked) {
    const statusColor = client.is_online ? 'var(--green)' : 'var(--text-dim)';
    const statusText = client.is_online ? 'Online' : 'Offline';
    const statusIcon = client.is_online ? '🟢' : '⚪';

    return `
        <div style="background:var(--bg-card);border:1px solid var(--border);border-radius:12px;padding:1.25rem;margin-bottom:0.75rem;">
            <div style="display:flex;justify-content:space-between;align-items:start;gap:1rem;">
                <div style="flex:1;">
                    <div style="display:flex;align-items:center;gap:0.5rem;margin-bottom:0.5rem;">
                        <h4 style="font-size:1rem;font-weight:600;color:var(--text);">${escapeHtml(client.client_name || 'Unnamed Client')}</h4>
                        <span style="font-size:0.75rem;color:${statusColor};display:flex;align-items:center;gap:0.25rem;">
                            ${statusIcon} ${statusText}
                        </span>
                    </div>
                    <div style="font-size:0.8rem;color:var(--text-dim);font-family:'JetBrains Mono',monospace;margin-bottom:0.25rem;">
                        ID: ${escapeHtml(client.client_id)}
                    </div>
                </div>
                <div style="display:flex;gap:0.5rem;">
                    ${isLinked ? `
                        <button onclick="unlinkClient('${escapeHtml(client.client_id)}')" class="btn btn-secondary" style="flex:0;white-space:nowrap;">
                            Unlink
                        </button>
                    ` : `
                        <button onclick="linkClient('${escapeHtml(client.client_id)}')" class="btn btn-primary" style="flex:0;white-space:nowrap;">
                            Link to My Account
                        </button>
                    `}
                </div>
            </div>
        </div>
    `;
}

async function linkClient(clientId) {
    try {
        const response = await fetch(`${API_BASE}/clients/${encodeURIComponent(clientId)}/link`, {
            method: 'POST',
            credentials: 'include'
        });
        const data = await response.json();

        if (data.success) {
            showToast('Client linked successfully!', 'success');
            loadClients(); // Reload the clients list
        } else {
            showToast(`Failed to link client: ${data.message}`, 'error');
        }
    } catch (error) {
        console.error('Failed to link client:', error);
        showToast('Failed to link client', 'error');
    }
}

async function unlinkClient(clientId) {
    if (!confirm('Are you sure you want to unlink this client? It will no longer receive downloads from your account.')) {
        return;
    }

    try {
        const response = await fetch(`${API_BASE}/clients/${encodeURIComponent(clientId)}/unlink`, {
            method: 'POST',
            credentials: 'include'
        });
        const data = await response.json();

        if (data.success) {
            showToast('Client unlinked successfully', 'success');
            loadClients(); // Reload the clients list
        } else {
            showToast(`Failed to unlink client: ${data.message}`, 'error');
        }
    } catch (error) {
        console.error('Failed to unlink client:', error);
        showToast('Failed to unlink client', 'error');
    }
}

// ─── System Health & Installation Assistant ───

async function loadSystemHealth() {
    await Promise.all([
        loadSystemInfo(),
        loadInstallationStats(),
        loadInstallationLogs()
    ]);
}

async function loadSystemInfo() {
    const container = document.getElementById('systemInfoContent');
    try {
        const response = await fetch(`${API_BASE}/system-info`);
        const data = await response.json();

        const statusColor = data.overall_status === 'Ready' ? 'var(--green)' :
                           data.overall_status === 'Warning' ? 'var(--gold)' : 'var(--red)';
        const statusIcon = data.overall_status === 'Ready' ? '✅' :
                          data.overall_status === 'Warning' ? '⚠️' : '❌';

        container.innerHTML = `
            <div style="display:grid;gap:1rem;">
                <div style="display:flex;align-items:center;gap:0.75rem;padding:1rem;background:var(--bg-surface);border-radius:8px;">
                    <div style="font-size:2rem;">${statusIcon}</div>
                    <div>
                        <div style="font-weight:600;color:${statusColor};font-size:1.1rem;">${data.overall_status}</div>
                        <div style="font-size:0.875rem;color:var(--text-muted);">System Status</div>
                    </div>
                </div>

                <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:0.75rem;">
                    <div style="background:var(--bg-surface);padding:1rem;border-radius:8px;">
                        <div style="font-size:0.75rem;color:var(--text-muted);margin-bottom:0.25rem;">RAM Available</div>
                        <div style="font-size:1.5rem;font-weight:700;font-family:'JetBrains Mono',monospace;">${data.ram_available_gb.toFixed(1)} GB</div>
                        <div style="font-size:0.7rem;color:var(--text-dim);">of ${data.ram_total_gb.toFixed(1)} GB</div>
                    </div>
                    <div style="background:var(--bg-surface);padding:1rem;border-radius:8px;">
                        <div style="font-size:0.75rem;color:var(--text-muted);margin-bottom:0.25rem;">Temp Space</div>
                        <div style="font-size:1.5rem;font-weight:700;font-family:'JetBrains Mono',monospace;">${data.temp_space_gb.toFixed(1)} GB</div>
                        <div style="font-size:0.7rem;color:var(--text-dim);">available</div>
                    </div>
                    <div style="background:var(--bg-surface);padding:1rem;border-radius:8px;">
                        <div style="font-size:0.75rem;color:var(--text-muted);margin-bottom:0.25rem;">CPU Cores</div>
                        <div style="font-size:1.5rem;font-weight:700;font-family:'JetBrains Mono',monospace;">${data.cpu_cores}</div>
                        <div style="font-size:0.7rem;color:var(--text-dim);">cores detected</div>
                    </div>
                    <div style="background:var(--bg-surface);padding:1rem;border-radius:8px;">
                        <div style="font-size:0.75rem;color:var(--text-muted);margin-bottom:0.25rem;">Antivirus</div>
                        <div style="font-size:1.5rem;font-weight:700;">${data.antivirus_active ? '🛡️' : '✅'}</div>
                        <div style="font-size:0.7rem;color:var(--text-dim);">${data.antivirus_active ? 'Active' : 'Inactive'}</div>
                    </div>
                </div>

                ${data.issues && data.issues.length > 0 ? `
                    <div style="background:var(--red-dim);border:1px solid rgba(239,68,68,0.25);border-radius:8px;padding:1rem;">
                        <div style="font-weight:600;margin-bottom:0.5rem;color:var(--red);">Issues Found:</div>
                        ${data.issues.map(issue => `<div style="font-size:0.875rem;margin-bottom:0.25rem;">• ${issue}</div>`).join('')}
                    </div>
                ` : ''}

                ${data.recommendations && data.recommendations.length > 0 ? `
                    <div style="background:var(--bg-surface);border-radius:8px;padding:1rem;">
                        <div style="font-weight:600;margin-bottom:0.5rem;">Recommendations:</div>
                        ${data.recommendations.map(rec => `<div style="font-size:0.875rem;color:var(--text-muted);margin-bottom:0.25rem;">• ${rec}</div>`).join('')}
                    </div>
                ` : ''}
            </div>
        `;
    } catch (error) {
        container.innerHTML = '<div style="color:var(--red);text-align:center;padding:2rem;">Failed to load system information</div>';
    }
}

async function loadInstallationStats() {
    const container = document.getElementById('installStatsContent');
    try {
        const response = await fetch(`${API_BASE}/installation/stats`);
        const stats = await response.json();

        container.innerHTML = `
            <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:0.75rem;">
                <div style="background:var(--bg-surface);padding:1rem;border-radius:8px;text-align:center;">
                    <div style="font-size:2rem;font-weight:700;font-family:'JetBrains Mono',monospace;">${stats.total_installs}</div>
                    <div style="font-size:0.75rem;color:var(--text-muted);margin-top:0.25rem;">Total Installs</div>
                </div>
                <div style="background:var(--green-dim);padding:1rem;border-radius:8px;text-align:center;">
                    <div style="font-size:2rem;font-weight:700;font-family:'JetBrains Mono',monospace;color:var(--green);">${stats.successful_installs}</div>
                    <div style="font-size:0.75rem;color:var(--text-muted);margin-top:0.25rem;">Successful</div>
                </div>
                <div style="background:var(--red-dim);padding:1rem;border-radius:8px;text-align:center;">
                    <div style="font-size:2rem;font-weight:700;font-family:'JetBrains Mono',monospace;color:var(--red);">${stats.failed_installs}</div>
                    <div style="font-size:0.75rem;color:var(--text-muted);margin-top:0.25rem;">Failed</div>
                </div>
                <div style="background:var(--bg-surface);padding:1rem;border-radius:8px;text-align:center;">
                    <div style="font-size:2rem;font-weight:700;font-family:'JetBrains Mono',monospace;">${stats.success_rate.toFixed(0)}%</div>
                    <div style="font-size:0.75rem;color:var(--text-muted);margin-top:0.25rem;">Success Rate</div>
                </div>
                <div style="background:var(--bg-surface);padding:1rem;border-radius:8px;text-align:center;">
                    <div style="font-size:2rem;font-weight:700;font-family:'JetBrains Mono',monospace;">${stats.avg_duration_minutes.toFixed(0)}<span style="font-size:1rem;color:var(--text-muted);">m</span></div>
                    <div style="font-size:0.75rem;color:var(--text-muted);margin-top:0.25rem;">Avg Duration</div>
                </div>
                <div style="background:var(--bg-surface);padding:1rem;border-radius:8px;text-align:center;">
                    <div style="font-size:2rem;font-weight:700;font-family:'JetBrains Mono',monospace;">${stats.avg_ram_usage_gb.toFixed(1)}<span style="font-size:1rem;color:var(--text-muted);">GB</span></div>
                    <div style="font-size:0.75rem;color:var(--text-muted);margin-top:0.25rem;">Avg RAM Usage</div>
                </div>
            </div>
        `;
    } catch (error) {
        container.innerHTML = '<div style="color:var(--red);text-align:center;padding:2rem;">Failed to load installation statistics</div>';
    }
}

async function loadInstallationLogs() {
    const container = document.getElementById('installLogsContent');
    // No dedicated endpoint for logs list — show a simple message
    container.innerHTML = '<div style="text-align:center;padding:2rem;color:var(--text-muted);">Installation logs are available per-game from the downloads view.</div>';
}

// Pre-Installation Check Modal
async function showPreInstallCheck(gameId) {
    document.getElementById('preInstallModal').classList.remove('hidden');
    const content = document.getElementById('preInstallContent');

    try {
        const response = await fetch(`${API_BASE}/pre-install-check/${gameId}`);
        const result = await response.json();

        const statusColor = result.overall_status === 'Pass' ? 'var(--green)' :
                           result.overall_status === 'Warning' ? 'var(--gold)' : 'var(--red)';
        const statusIcon = result.overall_status === 'Pass' ? '✅' :
                          result.overall_status === 'Warning' ? '⚠️' : '🚫';

        content.innerHTML = `
            <div style="text-align:center;padding:1rem;background:var(--bg-surface);border-radius:8px;margin-bottom:1rem;">
                <div style="font-size:3rem;margin-bottom:0.5rem;">${statusIcon}</div>
                <div style="font-size:1.5rem;font-weight:700;color:${statusColor};">${result.overall_status}</div>
                <div style="font-size:0.875rem;color:var(--text-muted);margin-top:0.25rem;">${result.can_proceed ? 'You can proceed with installation' : 'Please resolve issues before installing'}</div>
            </div>

            <div style="display:grid;gap:0.5rem;margin-bottom:1rem;">
                ${result.checks.map(check => {
                    const checkColor = check.status === 'Pass' ? 'var(--green)' :
                                     check.status === 'Warning' ? 'var(--gold)' : 'var(--red)';
                    const checkIcon = check.status === 'Pass' ? '✓' :
                                     check.status === 'Warning' ? '⚠' : '✗';
                    return `
                        <div style="display:flex;gap:0.75rem;padding:0.75rem;background:var(--bg-surface);border-radius:8px;">
                            <div style="color:${checkColor};font-weight:700;flex-shrink:0;">${checkIcon}</div>
                            <div>
                                <div style="font-weight:600;font-size:0.875rem;">${check.name}</div>
                                <div style="font-size:0.8rem;color:var(--text-muted);margin-top:0.25rem;">${check.message}</div>
                            </div>
                        </div>
                    `;
                }).join('')}
            </div>

            ${result.blockers && result.blockers.length > 0 ? `
                <div style="background:var(--red-dim);border:1px solid rgba(239,68,68,0.25);border-radius:8px;padding:1rem;margin-bottom:1rem;">
                    <div style="font-weight:700;color:var(--red);margin-bottom:0.5rem;">🚫 Blocking Issues:</div>
                    ${result.blockers.map(b => `<div style="font-size:0.875rem;margin-bottom:0.25rem;">• ${b}</div>`).join('')}
                </div>
            ` : ''}

            ${result.recommendations && result.recommendations.length > 0 ? `
                <div style="background:var(--bg-surface);border-radius:8px;padding:1rem;">
                    <div style="font-weight:600;margin-bottom:0.5rem;">💡 Recommendations:</div>
                    ${result.recommendations.map(r => `<div style="font-size:0.875rem;color:var(--text-muted);margin-bottom:0.25rem;">• ${r}</div>`).join('')}
                </div>
            ` : ''}

        `;
    } catch (error) {
        content.innerHTML = '<div style="color:var(--red);text-align:center;padding:2rem;">Failed to run pre-installation check</div>';
    }
}

function hidePreInstallModal() {
    document.getElementById('preInstallModal').classList.add('hidden');
}

// ─── Right Sidebar (Genre/Tag Filtering) ───

let sidebarGenres = [];
let sidebarTags = [];
let selectedSidebarGenres = new Set();
let selectedSidebarTags = new Set();

function toggleSidebar() {
    const sidebar = document.getElementById('rightSidebar');
    const toggle = document.getElementById('sidebarToggle');
    const isOpen = sidebar.classList.toggle('open');
    toggle.textContent = isOpen ? '✕' : '🔍';

    // Load sidebar data on first open
    if (isOpen && sidebarGenres.length === 0 && sidebarTags.length === 0) {
        loadSidebarGenres();
        loadSidebarTags();
    }
}

async function loadSidebarGenres() {
    try {
        const response = await fetch(`${API_BASE}/games/genres`);
        const data = await response.json();
        sidebarGenres = data.genres || [];
        renderSidebarGenres();
    } catch (error) {
        console.error('Failed to load sidebar genres:', error);
        document.getElementById('genreFilterList').innerHTML = '<div style="text-align:center;padding:1rem;color:var(--red);font-size:0.75rem;">Failed to load</div>';
    }
}

async function loadSidebarTags() {
    try {
        const response = await fetch(`${API_BASE}/games/tags`);
        const data = await response.json();
        sidebarTags = data.tags || [];
        renderSidebarTags();
    } catch (error) {
        console.error('Failed to load sidebar tags:', error);
        document.getElementById('tagFilterList').innerHTML = '<div style="text-align:center;padding:1rem;color:var(--red);font-size:0.75rem;">Failed to load</div>';
    }
}

function renderSidebarGenres(filter = '') {
    const container = document.getElementById('genreFilterList');
    const filtered = filter
        ? sidebarGenres.filter(g => g.name.toLowerCase().includes(filter.toLowerCase()))
        : sidebarGenres;

    if (filtered.length === 0) {
        container.innerHTML = '<div style="text-align:center;padding:1rem;color:var(--text-dim);font-size:0.75rem;">No genres found</div>';
        return;
    }

    container.innerHTML = filtered.map(genre => {
        const isActive = selectedSidebarGenres.has(genre.name);
        return `
            <div class="filter-item ${isActive ? 'active' : ''}" onclick="toggleSidebarGenre('${escapeHtml(genre.name)}')">
                <span>${escapeHtml(genre.name)}</span>
                <span class="filter-count">${genre.count}</span>
            </div>
        `;
    }).join('');
}

function renderSidebarTags(filter = '') {
    const container = document.getElementById('tagFilterList');
    const filtered = filter
        ? sidebarTags.filter(t => t.name.toLowerCase().includes(filter.toLowerCase()))
        : sidebarTags;

    if (filtered.length === 0) {
        container.innerHTML = '<div style="text-align:center;padding:1rem;color:var(--text-dim);font-size:0.75rem;">No tags found</div>';
        return;
    }

    container.innerHTML = filtered.map(tag => {
        const isActive = selectedSidebarTags.has(tag.name);
        return `
            <div class="filter-item ${isActive ? 'active' : ''}" onclick="toggleSidebarTag('${escapeHtml(tag.name)}')">
                <span>${escapeHtml(tag.name)}</span>
                <span class="filter-count">${tag.count}</span>
            </div>
        `;
    }).join('');
}

function filterSidebarGenres() {
    const search = document.getElementById('genreSidebarSearch').value;
    renderSidebarGenres(search);
}

function filterSidebarTags() {
    const search = document.getElementById('tagSidebarSearch').value;
    renderSidebarTags(search);
}

function toggleSidebarGenre(genre) {
    if (selectedSidebarGenres.has(genre)) {
        selectedSidebarGenres.delete(genre);
    } else {
        selectedSidebarGenres.add(genre);
    }
    renderSidebarGenres(document.getElementById('genreSidebarSearch').value);
    applyMultiFilter();
}

function toggleSidebarTag(tag) {
    if (selectedSidebarTags.has(tag)) {
        selectedSidebarTags.delete(tag);
    } else {
        selectedSidebarTags.add(tag);
    }
    renderSidebarTags(document.getElementById('tagSidebarSearch').value);
    applyMultiFilter();
}

function applyMultiFilter() {
    // For now, we'll apply genre filter from sidebar
    // Tag filtering will be added once we have tags in the database
    if (selectedSidebarGenres.size > 0) {
        // Use the first selected genre (we can enhance this later for multi-select)
        const firstGenre = Array.from(selectedSidebarGenres)[0];
        document.getElementById('genreSelect').value = firstGenre;
    } else {
        document.getElementById('genreSelect').value = '';
    }

    currentPage = 1;
    currentGames = [];
    loadGames();
}

// ─── Featured Games Carousel ───

let featuredGames = [];
let carouselIndex = 0;
let currentCategory = 'hot';

async function loadFeaturedCategory(category) {
    currentCategory = category;

    // Update active tab
    document.querySelectorAll('.carousel-tab').forEach(tab => {
        tab.classList.toggle('active', tab.dataset.category === category);
    });

    // Show loading state
    document.getElementById('carouselGrid').innerHTML = `
        <div style="grid-column:1/-1;display:flex;align-items:center;justify-content:center;padding:4rem;color:var(--text-dim);">
            <div class="spinner" style="width:48px;height:48px;border-width:4px;"></div>
        </div>
    `;

    try {
        const response = await fetch(`${API_BASE}/games/featured?category=${category}`);
        if (!response.ok) throw new Error('Failed to load featured games');

        featuredGames = await response.json();

        if (featuredGames.length === 0) {
            document.getElementById('carouselGrid').innerHTML = `
                <div style="grid-column:1/-1;display:flex;align-items:center;justify-content:center;padding:4rem;color:var(--text-dim);font-size:0.875rem;">
                    No games available
                </div>
            `;
            return;
        }

        renderCarousel();
    } catch (error) {
        console.error('Failed to load featured games:', error);
        document.getElementById('carouselGrid').innerHTML = `
            <div style="grid-column:1/-1;display:flex;align-items:center;justify-content:center;padding:4rem;color:var(--red);font-size:0.875rem;">
                Failed to load featured games
            </div>
        `;
    }
}

function renderCarousel() {
    if (featuredGames.length === 0) return;

    const showRank = currentCategory === 'hot' || currentCategory === 'top_week';
    const cardsToShow = featuredGames.slice(0, 6); // 2x3 grid

    const html = cardsToShow.map((game, index) => {
        const hasThumb = game.thumbnail_url && game.thumbnail_url.length > 0;
        const thumbContent = hasThumb
            ? `<img src="${escapeHtml(game.thumbnail_url)}" alt="${escapeHtml(game.title)}" loading="lazy">`
            : `<div style="display:flex;align-items:center;justify-content:center;height:100%;font-size:3rem;color:var(--text-dim);opacity:0.3;">🎮</div>`;

        const rankBadge = showRank ? `<div class="carousel-card-rank">#${index + 1}</div>` : '';

        return `
            <div class="carousel-card" onclick="showGameModal(${game.id})">
                <div class="carousel-card-thumb">
                    ${thumbContent}
                    ${rankBadge}
                </div>
                <div class="carousel-card-body">
                    <div class="carousel-card-title">${escapeHtml(game.title)}</div>
                    <div class="carousel-card-size">${escapeHtml(game.file_size)}</div>
                </div>
            </div>
        `;
    }).join('');

    document.getElementById('carouselGrid').innerHTML = html;
}

// Remove unused carousel navigation functions
function renderCarouselThumbs() {
    // No longer needed with grid layout
}

// ─── Notifications ───

let notificationPanelOpen = false;

function toggleNotificationPanel() {
    const panel = document.getElementById('notificationPanel');
    notificationPanelOpen = !notificationPanelOpen;

    if (notificationPanelOpen) {
        panel.classList.remove('hidden');
        loadNotifications();
    } else {
        panel.classList.add('hidden');
    }
}

// Close notification panel when clicking outside
document.addEventListener('click', (e) => {
    const panel = document.getElementById('notificationPanel');
    const btn = document.getElementById('notificationBtn');

    if (notificationPanelOpen && !panel.contains(e.target) && !btn.contains(e.target)) {
        panel.classList.add('hidden');
        notificationPanelOpen = false;
    }
});

async function loadNotifications() {
    try {
        const response = await fetch(`${API_BASE}/notifications`);
        const notifications = await response.json();

        const container = document.getElementById('notificationList');

        if (notifications.length === 0) {
            container.innerHTML = `
                <div style="text-align:center;padding:2rem;color:var(--text-dim);font-size:0.85rem;">
                    <div style="font-size:2rem;margin-bottom:0.5rem;opacity:0.5;">🔔</div>
                    <div>No notifications yet</div>
                </div>
            `;
            return;
        }

        container.innerHTML = notifications.map(notif => {
            const isUnread = !notif.read;
            const time = formatRelativeTime(notif.created_at);

            return `
                <div class="notification-item ${isUnread ? 'unread' : ''}" onclick="markNotificationRead(${notif.id})">
                    <div class="notification-title">${escapeHtml(notif.title)}</div>
                    <div class="notification-message">${escapeHtml(notif.message)}</div>
                    <div class="notification-time">${time}</div>
                </div>
            `;
        }).join('');
    } catch (error) {
        console.error('Failed to load notifications:', error);
        document.getElementById('notificationList').innerHTML = `
            <div style="text-align:center;padding:2rem;color:var(--red);font-size:0.85rem;">
                Failed to load notifications
            </div>
        `;
    }
}

async function markNotificationRead(id) {
    try {
        await fetch(`${API_BASE}/notifications/${id}/read`, { method: 'POST' });
        loadNotifications();
        updateNotificationBadge();
    } catch (error) {
        console.error('Failed to mark notification as read:', error);
    }
}

async function markAllNotificationsRead() {
    try {
        await fetch(`${API_BASE}/notifications/read-all`, { method: 'POST' });
        loadNotifications();
        updateNotificationBadge();
        showToast('All notifications marked as read', 'success');
    } catch (error) {
        console.error('Failed to mark all notifications as read:', error);
        showToast('Failed to mark notifications as read', 'error');
    }
}

async function updateNotificationBadge() {
    try {
        const response = await fetch(`${API_BASE}/notifications/count`);
        const data = await response.json();
        const count = data.count || 0;

        const badge = document.getElementById('notificationBadge');
        if (count > 0) {
            badge.textContent = count > 99 ? '99+' : count;
            badge.classList.remove('hidden');
        } else {
            badge.classList.add('hidden');
        }
    } catch (error) {
        console.error('Failed to update notification badge:', error);
    }
}

function startNotificationPolling() {
    // Poll every 30 seconds
    updateNotificationBadge();
    notificationPollInterval = setInterval(updateNotificationBadge, 30000);
}

function stopNotificationPolling() {
    if (notificationPollInterval) {
        clearInterval(notificationPollInterval);
        notificationPollInterval = null;
    }
}

function formatRelativeTime(dateStr) {
    try {
        const date = new Date(dateStr);
        const now = new Date();
        const diffMs = now - date;
        const diffSecs = Math.floor(diffMs / 1000);
        const diffMins = Math.floor(diffSecs / 60);
        const diffHours = Math.floor(diffMins / 60);
        const diffDays = Math.floor(diffHours / 24);

        if (diffSecs < 60) return 'Just now';
        if (diffMins < 60) return `${diffMins}m ago`;
        if (diffHours < 24) return `${diffHours}h ago`;
        if (diffDays < 7) return `${diffDays}d ago`;
        return date.toLocaleDateString();
    } catch {
        return dateStr;
    }
}

function showBrowserNotification(title, message) {
    if (!('Notification' in window)) return;
    if (Notification.permission !== 'granted') return;

    try {
        new Notification(title, {
            body: message,
            icon: '/favicon.ico',
            badge: '/favicon.ico'
        });
    } catch (error) {
        console.error('Failed to show browser notification:', error);
    }
}

