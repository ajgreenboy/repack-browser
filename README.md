# FitGirl Scraper - Self-Hosted Game Repack Browser

A **self-hosted, multi-user game repack browser** for home networks. Browse 6,600+ games from FitGirl Repacks and SteamRIP, download directly to your Windows PC, and install with one click.

> **⚠️ For Personal/Household Use Only**
>
> Designed for **home networks** where one admin sets up a Real-Debrid account and all household members (same IP) share it. Downloads happen on each user's own PC, not the server.

---

## 📚 Table of Contents

- [Overview](#-overview)
- [Features](#-features)
- [Architecture](#-architecture)
- [Quick Start](#-quick-start)
- [Configuration](#-configuration)
- [Usage Guide](#-usage-guide)
- [Development](#-development)
- [API Documentation](#-api-documentation)
- [Database Schema](#-database-schema)
- [Frontend Guide](#-frontend-guide)
- [Troubleshooting](#-troubleshooting)
- [Legal](#-legal)

---

## 🎯 Overview

### How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│  USER (Web Browser)                                             │
│  ├─ Browse 6,600+ games from FitGirl & SteamRIP                │
│  ├─ Search, filter, sort by genre/size/date                    │
│  ├─ View screenshots and details                               │
│  └─ Click "Download" button                                    │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ↓
┌─────────────────────────────────────────────────────────────────┐
│  SERVER (Docker Container)                                      │
│  ├─ Receives download request                                  │
│  ├─ Converts magnet link to direct URLs via Real-Debrid        │
│  ├─ Stores download in database (status: "pending")            │
│  └─ Waits for client to poll                                   │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ↓
┌─────────────────────────────────────────────────────────────────┐
│  WINDOWS CLIENT (Runs on User's PC)                            │
│  ├─ Polls server every 30 seconds                              │
│  ├─ Finds pending download                                     │
│  ├─ Downloads files to local disk (direct URLs)                │
│  ├─ Reports progress every 2 seconds (speed, ETA)              │
│  ├─ Extracts archives (ZIP, 7Z, RAR) using 7-Zip              │
│  ├─ Launches installer with UAC elevation                      │
│  └─ Reports completion to server                               │
└─────────────────────────────────────────────────────────────────┘
```

### Key Concepts

**Server-Side:**
- Hosts SQLite database with game catalog
- Serves web UI (vanilla JavaScript)
- Manages user authentication (session cookies)
- Integrates with Real-Debrid API (one account for household)
- Tracks download progress reported by clients

**Client-Side:**
- Runs as Windows application (Rust + eframe GUI)
- Polls server for pending downloads
- Downloads files using reqwest HTTP client
- Extracts archives using 7-Zip CLI
- Launches installers with ShellExecuteW + UAC
- Reports real-time progress via REST API

**Real-Debrid Integration:**
- Server admin sets up ONE Real-Debrid account
- All household members share it (allowed per RD TOS for same IP)
- Server converts magnet links to direct download URLs
- Client downloads from these direct URLs (no torrenting on client)

---

## ✨ Features

### 🎨 Hydra-Inspired UI

**Modern Desktop-App Aesthetic:**
- **Left Sidebar Navigation** - Always visible, quick access to Home/Catalogue/Library/Downloads
- **List View Default** - Horizontal game cards with thumbnails, metadata, and actions
- **Card View Toggle** - Switch to grid layout with localStorage persistence
- **2x3 Carousel Grid** - Featured games from FitGirl's Top 50/150 repacks
- **Library View** - Two tabs: "Favorites" (starred games) and "Downloaded" (completed)
- **Very Dark Theme** - Hydra-inspired color palette (#0a0a0a base)
- **Source Toggle** - Filter by All/FitGirl/SteamRIP from sidebar

### 🎮 Game Catalog (6,600+ Games)

**Data Sources:**
- **FitGirl Repacks** - Scraped via WordPress REST API (`/wp-json/wp/v2/posts`)
- **SteamRIP** - Scraped via WordPress REST API
- **Top Repacks** - FitGirl's `/top-50-repacks/` and `/top-150-repacks/` pages

**Metadata:**
- Title, file size, magnet link, genres
- Screenshot galleries (strict .jpg/.png/.webp for FitGirl, any HTTP URL for SteamRIP)
- Source link to original repack page
- Upload date and search index

**Search & Filter:**
- Full-text search on titles
- Genre filtering (Action, RPG, Strategy, etc.)
- Sort by date, size, or title
- Random game picker

### 👥 Multi-User Authentication

**Session-Based Auth:**
- Bcrypt password hashing
- HttpOnly, SameSite=Lax cookies
- 30-day session expiry
- Hourly cleanup task removes expired sessions

**User Roles:**
- **Admin** - Full access, can see all downloads
- **Regular** - Can only see own downloads and favorites

**Per-User Data:**
- Personal favorites (game_id + user_id in `favorites` table)
- Download history (filtered by user_id)
- Client registration (each client linked to user account)

### 📥 Smart Downloads

**Download Workflow:**
1. User clicks "Download" → Frontend validates client is online
2. Server calls Real-Debrid API to convert magnet → direct URLs
3. Server stores download in database (status: "pending")
4. Client polls `/api/downloads/queue/{client_id}` every 30 seconds
5. Client downloads files, reports progress every 2 seconds
6. Client extracts archives, launches installer
7. Client reports completion → Server updates status to "completed"

**Progress Tracking:**
- Real-time download speed (MB/s, KB/s)
- Estimated time remaining (ETA)
- Overall progress across multiple files
- Status: pending → downloading → extracting → installing → completed/failed

**Archive Extraction:**
- **ZIP** - Native Rust extraction via `zip` crate
- **7Z** - Uses `sevenz-rust` crate
- **RAR** - Uses 7-Zip CLI (`7z.exe x`) with multiple installation path checks

### 🪟 Windows Client Features

**Background Service:**
- Runs in system tray (minimizable)
- Polls server every 30 seconds
- eframe GUI for settings

**Download Manager:**
- Retry logic with exponential backoff (5s, 10s, 20s)
- Timeout: 30s connection, no overall timeout (large files)
- Streaming download with chunk processing
- Progress tracking per file and overall

**Auto-Extraction:**
- Sanitizes filenames (URL-decodes, removes Windows invalid chars)
- Creates game-specific subdirectories
- Verifies write permissions before extraction
- Supports nested archives

**Installer Integration:**
- Finds installer (setup.exe, install.exe, installer.exe)
- Prompts UAC elevation using ShellExecuteW with "runas" verb
- Launches normal installer UI (no silent mode)
- Polls tasklist to detect completion (max 30 min timeout)

**Window Settings:**
- 500x400px default size
- Decorated windows (title bar, borders)
- Taskbar integration
- Resizable, not maximized on launch

**Default Locations:**
- Downloads: `%USERPROFILE%\Downloads\Games` (no admin required)
- Config: `%APPDATA%\RepackClient\config.toml`

---

## 🏗️ Architecture

### Technology Stack

**Server (Rust):**
```
axum (web framework)
  ├─ tower-http (CORS, logging)
  ├─ sqlx (SQLite, async)
  ├─ bcrypt (password hashing)
  ├─ reqwest (HTTP client for RD API)
  ├─ serde/serde_json (serialization)
  └─ tokio (async runtime)
```

**Client (Rust):**
```
eframe (GUI framework)
  ├─ egui (immediate mode GUI)
  ├─ tokio (async runtime)
  ├─ reqwest (HTTP client)
  ├─ zip/sevenz-rust (extraction)
  ├─ winapi (Windows APIs)
  └─ tray-item (system tray)
```

**Frontend (Vanilla JS):**
```
No frameworks!
  ├─ Fetch API (credentials: 'include')
  ├─ CSS Grid/Flexbox
  ├─ localStorage (view mode persistence)
  └─ CommonMark (markdown rendering)
```

### Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  FRONTEND (frontend/)                                           │
│  ├─ index.html - Main UI shell                                 │
│  ├─ app.js - Core application logic                            │
│  │   ├─ fetchGames() - Loads and renders game list            │
│  │   ├─ renderCarousel() - 2x3 featured games grid            │
│  │   ├─ openGameModal() - Game details popup                  │
│  │   ├─ queueDownload() - Initiates download flow             │
│  │   └─ Global fetch override (adds credentials)              │
│  └─ styles.css - Hydra-inspired theming                        │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  SERVER (src/)                                                  │
│  ├─ main.rs - Axum router, endpoints                           │
│  │   ├─ /api/games - Game catalog                             │
│  │   ├─ /api/games/featured - Top 50/150                      │
│  │   ├─ /api/downloads - Download management                  │
│  │   ├─ /api/auth - Login/logout                              │
│  │   └─ /api/clients - Client registration                    │
│  ├─ db.rs - SQLite schema + queries                            │
│  ├─ auth.rs - Session management                               │
│  ├─ scrapers/ - Game data collection                           │
│  │   ├─ fitgirl.rs - FitGirl scraper                          │
│  │   ├─ steamrip.rs - SteamRIP scraper                        │
│  │   └─ utils.rs - Shared utilities                           │
│  └─ client_downloads.rs - RD integration                       │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  CLIENT (client-agent/src/)                                     │
│  ├─ main.rs - GUI + initialization                             │
│  ├─ download_processor.rs - Main download workflow             │
│  ├─ downloader.rs - HTTP file download                         │
│  ├─ extractor.rs - Archive extraction                          │
│  ├─ server_client.rs - API client                              │
│  ├─ config.rs - TOML configuration                             │
│  └─ system_info.rs - Hardware detection                        │
└─────────────────────────────────────────────────────────────────┘
```

### Data Flow

**Scraping (Initial Setup):**
```
Admin clicks "Scrape" button
  ↓
Server calls FitGirl/SteamRIP WordPress API
  ↓
Parse HTML content (markdown to text, extract images)
  ↓
Insert into games table
  ↓
Scrape Top 50/150 pages (match titles to game IDs)
  ↓
Insert into game_categories table
```

**Download Flow (Detailed):**
```
1. USER ACTION
   - User opens game modal
   - Clicks "Download" button
   - Frontend: checks client.online = true

2. FRONTEND REQUEST
   POST /api/downloads/queue
   Body: { game_id: 123, client_id: "uuid" }
   Credentials: include (sends session cookie)

3. SERVER PROCESSING
   - Validate session, get user_id
   - Fetch game from database
   - Call Real-Debrid API: POST /torrents/addMagnet
   - Get torrent ID, select all files
   - Call RD API: POST /torrents/unrestrict
   - Get direct download URLs
   - Insert into downloads table:
     {
       user_id, game_id, client_id,
       status: "pending",
       direct_urls: ["url1", "url2"],
       created_at: timestamp
     }

4. CLIENT POLLING (every 30s)
   GET /api/downloads/queue/{client_id}
   - Server filters by client_id + status="pending"
   - Returns list of pending downloads

5. CLIENT DOWNLOAD
   For each file:
     - Download with retry (3 attempts, exponential backoff)
     - Report progress every 2s:
       POST /api/downloads/{id}/progress
       Body: { status, progress, download_speed, eta }

6. CLIENT EXTRACTION
   - Create game subdirectory
   - Extract ZIP/7Z/RAR using appropriate tool
   - Update status to "extracting"

7. CLIENT INSTALLATION
   - Find installer (setup.exe, install.exe)
   - Launch with ShellExecuteW + UAC prompt
   - Poll tasklist until installer exits
   - Update status to "completed"
```

---

## 🚀 Quick Start

### Prerequisites

- **Server:** Docker, Docker Compose
- **Client:** Windows 10/11, 7-Zip installed
- **Account:** Real-Debrid subscription (one per household)

### Step 1: Server Setup

```bash
# Clone repository
git clone https://github.com/ajgreenboy/repack-browser.git
cd repack-browser

# Set Real-Debrid API key
export RD_API_KEY="your_api_key_here"

# Optional: Set RAWG API key for metadata
export RAWG_API_KEY="your_rawg_key_here"

# Build and start
docker compose build --no-cache
docker compose up -d

# Check logs
docker compose logs -f
```

**Access UI:** `http://localhost:3030`

**First Login:**
- Username: `admin`
- Password: `admin`
- **CHANGE IMMEDIATELY!**

**Initial Setup:**
1. Click "Settings" (gear icon)
2. Add Real-Debrid API key (get from https://real-debrid.com/apitoken)
3. Optionally add RAWG API key (get from https://rawg.io/apidocs)
4. Click "Scrape" button
5. Wait ~5 minutes for 6,600+ games to populate

### Step 2: Windows Client Setup

**Download:**
- Get `repack-client-windows-x64.exe` from [Releases](https://github.com/ajgreenboy/repack-browser/releases)
- Or build from source (see Development section)

**Install 7-Zip:**
```powershell
# Required for RAR extraction
winget install 7zip.7zip
# Or download from https://www.7-zip.org/
```

**Configure:**

1. Run `repack-client.exe` (generates config at `%APPDATA%\RepackClient\config.toml`)
2. Edit config:

```toml
[server]
url = "http://192.168.1.100:3030"  # Your server IP
enabled = true
poll_interval_secs = 30

[extraction]
output_dir = "C:\\Users\\YourName\\Downloads\\Games"
delete_after_extract = false
```

3. Restart client
4. Minimize to system tray (keep running)

**Verify:**
- Check server UI - "Clients" section should show your PC
- Client status should be "online"

### Step 3: Create User Accounts

1. Admin creates accounts for each household member
2. Or use self-registration (if enabled)
3. Each user logs in from their PC
4. Client auto-links to logged-in user

---

## ⚙️ Configuration

### Server Environment Variables

```bash
# Database (SQLite)
DATABASE_PATH=sqlite:/app/data/games.db?mode=rwc

# Real-Debrid (REQUIRED)
RD_API_KEY=your_real_debrid_api_key

# RAWG.io (Optional - for game metadata)
RAWG_API_KEY=your_rawg_api_key

# Server Port
PORT=3030

# Log Level
RUST_LOG=info  # debug, info, warn, error
```

### Client Configuration Reference

**Full config.toml:**

```toml
[client]
# Auto-generated UUID for this client
id = "550e8400-e29b-41d4-a716-446655440000"
# Hostname of this PC
name = "DESKTOP-ABC123"

[server]
# Server URL (must include protocol and port)
url = "http://192.168.1.100:3030"
# Enable/disable server polling
enabled = true
# Poll interval in seconds (recommended: 30)
poll_interval_secs = 30

[realdebrid]
# NOT USED - Server handles all RD operations
api_key = ""
enabled = false

[extraction]
# Where to download and extract games
# Default: %USERPROFILE%\Downloads\Games
output_dir = "C:\\Users\\YourName\\Downloads\\Games"

# Watch directory (legacy, not used)
watch_dir = "C:\\Users\\YourName\\Downloads"

# Delete archives after extraction
delete_after_extract = false

# Verify MD5 checksums (not implemented)
verify_md5 = true

[monitoring]
# How often to report progress during downloads (seconds)
report_interval_secs = 2

# Track RAM usage (for future features)
track_ram_usage = true
```

### Docker Compose Override

Create `docker-compose.override.yml`:

```yaml
services:
  fitgirl-browser:
    environment:
      - RD_API_KEY=your_key_here
      - RAWG_API_KEY=your_key_here
    ports:
      - "8080:3030"  # Change external port
    volumes:
      - ./custom-data:/app/data  # Custom data location
```

---

## 📖 Usage Guide

### Browsing Games

**Navigation:**
- **Home** - Featured carousel + recent games
- **Catalogue** - Full game list with search/filter
- **Library** - Your favorites and downloaded games
- **Downloads** - Active downloads with progress

**Searching:**
- Type in search box (live search as you type)
- Filter by genre (dropdown)
- Sort by date, size, or title
- Toggle source (All/FitGirl/SteamRIP)

**Game Details:**
- Click any game card to open modal
- View screenshots (click to enlarge)
- See file size, genres, upload date
- Visit source link to original repack page

### Downloading Games

**Before You Start:**
1. Ensure Windows client is running on your PC
2. Check client status shows "online" in UI
3. Verify you have enough disk space

**Download Process:**

1. **Find Game:**
   - Browse or search for game
   - Click game card to open details

2. **Start Download:**
   - Click green "Download" button
   - UI validates client connection
   - Shows success notification

3. **Monitor Progress:**
   - Go to "Downloads" view
   - See real-time progress bar
   - Download speed and ETA displayed
   - Status updates: downloading → extracting → installing

4. **Installation:**
   - UAC prompt appears (click "Yes")
   - Normal installer UI opens
   - Follow installation wizard
   - Choose install directory and options

5. **Completion:**
   - Desktop notification appears
   - Status changes to "completed"
   - Game ready to play!

**Retry Failed Downloads:**
- Click "Retry" button on failed download
- Client will re-attempt from beginning

**Cancel Downloads:**
- Click "Cancel" button
- Partial files remain in download folder

### Managing Favorites

**Add to Favorites:**
- Open game details modal
- Click star icon (⭐)
- Game saved to your personal favorites

**View Favorites:**
- Navigate to Library → Favorites tab
- See all starred games
- Click to view details or download

### Managing Downloads

**View Download History:**
- Navigate to Downloads view
- Filter: All / In Progress / Completed / Failed
- Admin sees all downloads, users see own only

**Clear Completed:**
- Click "Clear Completed" button
- Removes successful downloads from list

---

## 🛠️ Development

### Building from Source

**Server (Docker):**
```bash
# Development build
docker compose build

# Production build with optimizations
docker compose build --no-cache

# Run with logs
docker compose up
```

**Client (Windows cross-compile from Linux):**
```bash
# Install Rust cross-compilation toolchain
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64

# Build
cd client-agent
cargo build --release --target x86_64-pc-windows-gnu

# Output: target/x86_64-pc-windows-gnu/release/repack-client.exe
```

**Client (Native Windows build):**
```powershell
# Install Rust from https://rustup.rs
rustup default stable

# Build
cd client-agent
cargo build --release

# Output: target/release/repack-client.exe
```

### Project Structure

```
repack-browser/
├── src/                          # Server source (Rust)
│   ├── main.rs                   # Axum router + endpoints
│   ├── db.rs                     # Database schema + queries
│   ├── auth.rs                   # Session management
│   ├── client_downloads.rs       # Real-Debrid integration
│   └── scrapers/
│       ├── mod.rs                # GameScraper trait
│       ├── fitgirl.rs            # FitGirl scraper
│       ├── steamrip.rs           # SteamRIP scraper
│       └── utils.rs              # Shared utilities
│
├── frontend/                     # Web UI
│   ├── index.html                # Main HTML shell
│   ├── app.js                    # Application logic
│   └── assets/                   # Images, icons
│
├── client-agent/                 # Windows client
│   ├── src/
│   │   ├── main.rs               # GUI + initialization
│   │   ├── download_processor.rs # Download workflow
│   │   ├── downloader.rs         # HTTP download
│   │   ├── extractor.rs          # Archive extraction
│   │   ├── server_client.rs      # API client
│   │   └── config.rs             # TOML config
│   └── Cargo.toml
│
├── releases/                     # Pre-built binaries
│   └── repack-client-windows-x64.exe
│
├── data/                         # Runtime data (gitignored)
│   └── games.db                  # SQLite database
│
├── docker-compose.yml            # Docker setup
├── Dockerfile                    # Server container
└── README.md
```

### Adding Features

**New Scraper:**

1. Create `src/scrapers/newsource.rs`
2. Implement `GameScraper` trait:
```rust
pub struct NewSourceScraper {
    client: Client,
}

#[async_trait]
impl GameScraper for NewSourceScraper {
    async fn scrape_games(&self) -> Result<Vec<Game>, Box<dyn Error + Send + Sync>> {
        // Scraping logic
    }

    fn source_name(&self) -> &'static str {
        "newsource"
    }
}
```

3. Register in `main.rs`:
```rust
scrapers.push(Box::new(NewSourceScraper::new()));
```

**New API Endpoint:**

1. Add route in `main.rs`:
```rust
let app = Router::new()
    .route("/api/new-feature", get(new_feature_handler))
    .layer(/* middlewares */);
```

2. Implement handler:
```rust
async fn new_feature_handler(
    State(state): State<AppState>,
) -> Result<Json<Response>, StatusCode> {
    // Handler logic
}
```

**Frontend Feature:**

1. Add UI in `index.html`
2. Add logic in `app.js`:
```javascript
async function newFeature() {
    const response = await fetch('/api/new-feature', {
        credentials: 'include'
    });
    const data = await response.json();
    // Update UI
}
```

### Running Tests

```bash
# Server tests
cd repack-browser
cargo test

# Client tests
cd client-agent
cargo test

# Integration tests
docker compose -f docker-compose.test.yml up
```

---

## 📡 API Documentation

### Authentication

**Login:**
```http
POST /api/auth/login
Content-Type: application/json

{
  "username": "admin",
  "password": "password"
}

Response: 200 OK
Set-Cookie: session_id=...; HttpOnly; SameSite=Lax
{
  "success": true,
  "is_admin": true,
  "username": "admin"
}
```

**Logout:**
```http
POST /api/auth/logout
Cookie: session_id=...

Response: 200 OK
```

**Check Session:**
```http
GET /api/auth/check
Cookie: session_id=...

Response: 200 OK
{
  "authenticated": true,
  "username": "admin",
  "is_admin": true
}
```

### Games API

**List Games:**
```http
GET /api/games?search=witcher&genre=rpg&sort=date&source=fitgirl&limit=50&offset=0
Cookie: session_id=...

Response: 200 OK
{
  "games": [
    {
      "id": 1,
      "title": "The Witcher 3",
      "file_size": "35 GB",
      "magnet_link": "magnet:?xt=...",
      "search_title": "witcher 3",
      "genres": "Action, RPG",
      "thumbnail_url": "https://...",
      "source": "fitgirl",
      "source_link": "https://fitgirl-repacks.site/...",
      "created_at": "2026-01-15T10:30:00Z"
    }
  ],
  "total": 1,
  "limit": 50,
  "offset": 0
}
```

**Get Featured Games:**
```http
GET /api/games/featured?category=hot
Cookie: session_id=...

Response: 200 OK
[
  {
    "id": 1,
    "title": "Game Title",
    "file_size": "50 GB",
    "thumbnail_url": "https://...",
    "source": "fitgirl",
    "genres": "Action",
    "created_at": "2026-01-15T10:30:00Z"
  }
]
```

Categories: `hot`, `top_week`, `recent`

**Get Game Details:**
```http
GET /api/games/1
Cookie: session_id=...

Response: 200 OK
{
  "id": 1,
  "title": "The Witcher 3",
  "file_size": "35 GB",
  "magnet_link": "magnet:?xt=...",
  "search_title": "witcher 3",
  "genres": "Action, RPG",
  "thumbnail_url": "https://...",
  "images": ["https://...", "https://..."],
  "source": "fitgirl",
  "source_link": "https://fitgirl-repacks.site/...",
  "created_at": "2026-01-15T10:30:00Z"
}
```

**Batch Get Games:**
```http
GET /api/games?ids=1,2,3
Cookie: session_id=...

Response: 200 OK
{
  "games": [...],
  "total": 3
}
```

### Downloads API

**Queue Download:**
```http
POST /api/downloads/queue
Cookie: session_id=...
Content-Type: application/json

{
  "game_id": 1,
  "client_id": "550e8400-e29b-41d4-a716-446655440000"
}

Response: 200 OK
{
  "success": true,
  "download_id": 42
}
```

**Get Download Queue (Client):**
```http
GET /api/downloads/queue/550e8400-e29b-41d4-a716-446655440000
Cookie: session_id=...

Response: 200 OK
[
  {
    "id": 42,
    "game_id": 1,
    "game_title": "The Witcher 3",
    "game_size": "35 GB",
    "magnet_link": "magnet:?xt=...",
    "direct_urls": [
      "https://rdl.real-debrid.com/...",
      "https://rdl.real-debrid.com/..."
    ],
    "status": "pending",
    "progress": 0.0,
    "download_speed": null,
    "eta": null,
    "error_message": null,
    "created_at": "2026-02-07T20:00:00Z"
  }
]
```

**Update Download Progress:**
```http
POST /api/downloads/42/progress
Cookie: session_id=...
Content-Type: application/json

{
  "status": "downloading",
  "progress": 45.5,
  "download_speed": "12.5 MB/s",
  "eta": "5m 30s",
  "error_message": null
}

Response: 200 OK
```

**List Downloads (User):**
```http
GET /api/downloads?status=all&limit=50
Cookie: session_id=...

Response: 200 OK
[
  {
    "id": 42,
    "game_id": 1,
    "game_title": "The Witcher 3",
    "status": "completed",
    "progress": 100.0,
    "created_at": "2026-02-07T20:00:00Z",
    "updated_at": "2026-02-07T21:30:00Z"
  }
]
```

Status filter: `all`, `pending`, `downloading`, `completed`, `failed`

### Clients API

**Register Client:**
```http
POST /api/clients/register
Cookie: session_id=...
Content-Type: application/json

{
  "client_id": "550e8400-e29b-41d4-a716-446655440000",
  "client_name": "DESKTOP-ABC123",
  "os_info": "Windows 10 Pro",
  "ip_address": "192.168.1.50"
}

Response: 200 OK
{
  "success": true
}
```

**Heartbeat:**
```http
POST /api/clients/550e8400-e29b-41d4-a716-446655440000/heartbeat
Cookie: session_id=...

Response: 200 OK
```

**List Clients:**
```http
GET /api/clients
Cookie: session_id=... (admin only)

Response: 200 OK
[
  {
    "client_id": "550e8400-e29b-41d4-a716-446655440000",
    "client_name": "DESKTOP-ABC123",
    "user_id": 1,
    "os_info": "Windows 10 Pro",
    "last_seen": "2026-02-07T22:00:00Z",
    "status": "online"
  }
]
```

Status: `online` (< 2 minutes), `offline` (>= 2 minutes)

### Favorites API

**Add Favorite:**
```http
POST /api/favorites
Cookie: session_id=...
Content-Type: application/json

{
  "game_id": 1
}

Response: 200 OK
```

**Remove Favorite:**
```http
DELETE /api/favorites/1
Cookie: session_id=...

Response: 200 OK
```

**List Favorites:**
```http
GET /api/favorites
Cookie: session_id=...

Response: 200 OK
[
  {
    "id": 1,
    "title": "The Witcher 3",
    "file_size": "35 GB",
    "thumbnail_url": "https://...",
    "source": "fitgirl"
  }
]
```

---

## 🗄️ Database Schema

### Tables

**users:**
```sql
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    is_admin BOOLEAN NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

**sessions:**
```sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);
```

**games:**
```sql
CREATE TABLE games (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    file_size TEXT,
    magnet_link TEXT,
    search_title TEXT,
    genres TEXT,
    thumbnail_url TEXT,
    images TEXT,
    source TEXT NOT NULL,
    source_link TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_games_source ON games(source);
CREATE INDEX idx_games_search_title ON games(search_title);
CREATE INDEX idx_games_created_at ON games(created_at DESC);
```

**game_categories:**
```sql
CREATE TABLE game_categories (
    game_id INTEGER NOT NULL,
    category TEXT NOT NULL,
    rank INTEGER,
    scraped_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (game_id, category),
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE
);
CREATE INDEX idx_game_categories_category ON game_categories(category, rank);
```

Categories: `top_50`, `top_150`

**downloads:**
```sql
CREATE TABLE downloads (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    game_id INTEGER NOT NULL,
    client_id TEXT NOT NULL,
    magnet_link TEXT NOT NULL,
    direct_urls TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    progress REAL NOT NULL DEFAULT 0.0,
    download_speed TEXT,
    eta TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id),
    FOREIGN KEY (game_id) REFERENCES games(id)
);
CREATE INDEX idx_downloads_user_id ON downloads(user_id);
CREATE INDEX idx_downloads_client_id ON downloads(client_id);
CREATE INDEX idx_downloads_status ON downloads(status);
```

Status: `pending`, `downloading`, `extracting`, `installing`, `completed`, `failed`

**favorites:**
```sql
CREATE TABLE favorites (
    user_id INTEGER NOT NULL,
    game_id INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, game_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE
);
CREATE INDEX idx_favorites_user_id ON favorites(user_id);
```

**clients:**
```sql
CREATE TABLE clients (
    client_id TEXT PRIMARY KEY,
    client_name TEXT NOT NULL,
    user_id INTEGER,
    os_info TEXT,
    ip_address TEXT,
    last_seen TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    registered_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id)
);
CREATE INDEX idx_clients_user_id ON clients(user_id);
CREATE INDEX idx_clients_last_seen ON clients(last_seen DESC);
```

---

## 🎨 Frontend Guide

### Architecture

**No Framework - Vanilla JavaScript:**
- Direct DOM manipulation
- Fetch API with `credentials: 'include'`
- Event delegation for dynamic content
- localStorage for client-side state

### Key Functions

**Initialization:**
```javascript
document.addEventListener('DOMContentLoaded', async () => {
    await checkAuth();         // Verify session
    await loadGenres();        // Populate genre filter
    showView('games');         // Load default view
    startPolling();            // Begin periodic updates
});
```

**Global Fetch Override:**
```javascript
const originalFetch = window.fetch;
window.fetch = function(...args) {
    if (!args[1]) args[1] = {};
    args[1].credentials = 'include';  // Always send cookies
    return originalFetch.apply(this, args);
};
```

**View Management:**
```javascript
function showView(viewName) {
    // Hide all views
    document.querySelectorAll('.view').forEach(v => v.classList.add('hidden'));

    // Show requested view
    document.getElementById(viewName + 'View').classList.remove('hidden');

    // Update sidebar active state
    document.querySelectorAll('.sidebar-nav-item').forEach(btn => {
        btn.classList.toggle('active', btn.onclick.toString().includes(viewName));
    });

    // Load view data
    if (viewName === 'games') fetchGames();
    if (viewName === 'library') loadLibraryFavorites();
    if (viewName === 'downloads') loadDownloads();
}
```

**Game Rendering:**
```javascript
function renderGames(games) {
    const container = document.getElementById('gamesContainer');

    if (viewMode === 'list') {
        // Render list view
        container.className = 'game-list';
        container.innerHTML = games.map(game => `
            <div class="game-list-item" onclick="openGameModal(${game.id})">
                <img src="${game.thumbnail_url}" />
                <div class="game-list-info">
                    <h3>${game.title}</h3>
                    <div class="game-list-meta">
                        <span class="source-badge ${game.source}">${game.source}</span>
                        <span>${game.file_size}</span>
                    </div>
                </div>
                <button onclick="queueDownload(${game.id}, event)">Download</button>
            </div>
        `).join('');
    } else {
        // Render card view
        container.className = 'game-grid';
        container.innerHTML = games.map(game => `
            <div class="game-card" onclick="openGameModal(${game.id})">
                <img src="${game.thumbnail_url}" />
                <div class="game-card-body">
                    <h3>${game.title}</h3>
                    <p>${game.file_size}</p>
                </div>
            </div>
        `).join('');
    }
}
```

**Download Flow:**
```javascript
async function queueDownload(gameId, event) {
    event?.stopPropagation();

    // Validate client connection
    const clientStatus = await fetch('/api/clients/status');
    const { online } = await clientStatus.json();

    if (!online) {
        showNotification('Client offline', 'error');
        return;
    }

    // Queue download
    const response = await fetch('/api/downloads/queue', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ game_id: gameId, client_id: clientId })
    });

    if (response.ok) {
        showNotification('Download queued successfully!', 'success');
        showView('downloads');  // Switch to downloads view
    } else {
        showNotification('Failed to queue download', 'error');
    }
}
```

**Carousel:**
```javascript
async function renderCarousel(category = 'hot') {
    const response = await fetch(`/api/games/featured?category=${category}`);
    const games = await response.json();

    const grid = document.getElementById('carouselGrid');
    grid.innerHTML = games.slice(0, 6).map((game, index) => `
        <div class="carousel-card" onclick="openGameModal(${game.id})">
            <div class="carousel-card-thumb">
                <img src="${game.thumbnail_url}" style="object-fit:cover" />
                ${category !== 'recent' ? `<div class="carousel-card-rank">#${index + 1}</div>` : ''}
            </div>
            <div class="carousel-card-body">
                <div class="carousel-card-title">${game.title}</div>
                <div class="carousel-card-size">${game.file_size}</div>
            </div>
        </div>
    `).join('');
}
```

### Theming

**CSS Variables:**
```css
:root {
    /* Hydra Dark Theme */
    --bg-deepest: #0a0a0a;
    --bg-deep: #0f0f0f;
    --bg-card: #141414;
    --bg-card-hover: #1a1a1a;
    --bg-surface: #1c1c1c;
    --bg-input: #121212;

    /* Borders */
    --border: #2a2a2a;
    --border-hover: #3a3a3a;

    /* Text */
    --text-primary: #e5e7eb;
    --text-secondary: #9ca3af;
    --text-tertiary: #6b7280;

    /* Accent */
    --accent: #3b82f6;
    --accent-hover: #2563eb;

    /* Status */
    --success: #10b981;
    --error: #ef4444;
    --warning: #f59e0b;
}
```

---

## 🐛 Troubleshooting

### Common Issues

**1. "OS error 13: Permission denied"**

**Cause:** Client trying to write to protected directory (e.g., `C:\Games` from non-admin user)

**Solution:**
```toml
# Edit %APPDATA%\RepackClient\config.toml
[extraction]
output_dir = "C:\\Users\\YourName\\Downloads\\Games"
```

**2. "7-Zip not found" during RAR extraction**

**Cause:** 7-Zip not installed or not in standard location

**Solution:**
```powershell
# Install 7-Zip
winget install 7zip.7zip

# Or download from https://www.7-zip.org/
# Client checks these paths:
# C:\Program Files\7-Zip\7z.exe
# C:\Program Files (x86)\7-Zip\7z.exe
# 7z.exe in PATH
```

**3. "Client offline" when clicking Download**

**Cause:** Client not running or can't connect to server

**Solution:**
- Start `repack-client.exe`
- Check system tray for client icon
- Verify `server.url` in config
- Check firewall rules
- Test: `curl http://your-server:3030/api/health`

**4. Installer appears in weird fullscreen mode**

**Cause:** This was a bug in older versions (now fixed)

**Solution:**
- Update to latest client from Releases
- Current version launches installer with normal UI

**5. Downloads stuck at "pending"**

**Cause:** Real-Debrid API issue or invalid magnet link

**Solution:**
- Check server logs: `docker compose logs | grep -i "real-debrid"`
- Verify RD API key is valid: https://real-debrid.com/apitoken
- Check RD account has active subscription
- Try different game (magnet link might be dead)

**6. Session expires frequently**

**Cause:** Cookie settings or clock skew

**Solution:**
```bash
# Server side - check session cleanup
docker compose logs | grep -i "session cleanup"

# Browser - check cookie settings
# Ensure cookies enabled for site
# Check browser clock is correct
```

**7. Games not appearing after scrape**

**Cause:** Scraper errors or WordPress API changes

**Solution:**
```bash
# Check scraper logs
docker compose logs | grep -i "scraper"

# Manual rescrape
curl -X POST http://localhost:3030/api/rescrape?source=fitgirl
curl -X POST http://localhost:3030/api/rescrape?source=steamrip

# Check database
docker exec -it fitgirl-browser sqlite3 /app/data/games.db
sqlite> SELECT COUNT(*) FROM games;
sqlite> SELECT * FROM games LIMIT 5;
```

### Debug Mode

**Server:**
```bash
# Enable debug logging
export RUST_LOG=debug
docker compose up

# Or in docker-compose.yml:
environment:
  - RUST_LOG=debug
```

**Client:**
```bash
# Run with console output
repack-client.exe --verbose

# Check logs
# Location: %APPDATA%\RepackClient\logs\
```

### Performance Issues

**Slow UI:**
- Too many games loaded at once
- Solution: Implement pagination (currently loads all games)

**High Memory Usage:**
- Large carousel images
- Solution: Use thumbnails, lazy loading

**Slow Downloads:**
- Real-Debrid throttling
- Solution: Check RD account limits, premium vs free tier

---

## ⚖️ Legal

### Disclaimer

This application is for **educational and personal use only**. It does not:
- Host copyrighted content
- Distribute pirated games
- Facilitate illegal activity

It provides a browser interface for publicly available information and integrates with legitimate services (Real-Debrid).

**Users are responsible for:**
- Complying with local laws
- Respecting intellectual property
- Using legitimate accounts (Real-Debrid subscription)

**We strongly encourage:**
- Supporting game developers
- Purchasing games legally
- Using this tool for backup/archival purposes only

### Privacy

**Data Collection:**
- Username, password hash (bcrypt)
- Session cookies (HttpOnly, 30-day expiry)
- Download history (game IDs, timestamps, status)
- Client metadata (OS info, IP address, hostname)

**Data Sharing:**
- NO data shared with third parties
- Real-Debrid API calls use your account (server-side only)
- All data stored locally in SQLite database

**User Rights:**
- Request account deletion (removes all user data)
- Export download history (JSON format)
- Clear session cookies (logout)

### Credits

**Data Sources:**
- [FitGirl Repacks](https://fitgirl-repacks.site) - Game repacks
- [SteamRIP](https://steamrip.com) - Game repacks
- [RAWG.io](https://rawg.io) - Game metadata

**Services:**
- [Real-Debrid](https://real-debrid.com) - Download conversion

**Technologies:**
- [Rust](https://rust-lang.org) - Programming language
- [Axum](https://github.com/tokio-rs/axum) - Web framework
- [eframe](https://github.com/emilk/egui) - GUI framework
- [SQLite](https://sqlite.org) - Database

### License

MIT License - See [LICENSE](LICENSE) file

```
Copyright (c) 2026 FitGirl Scraper Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
```

---

## 🔗 Links

- **GitHub Repository:** https://github.com/ajgreenboy/repack-browser
- **Issues & Bugs:** https://github.com/ajgreenboy/repack-browser/issues
- **Releases:** https://github.com/ajgreenboy/repack-browser/releases
- **Real-Debrid:** https://real-debrid.com
- **RAWG API:** https://rawg.io/apidocs

---

## 📊 Stats

- **Games:** 6,600+ (FitGirl + SteamRIP combined)
- **Database Size:** ~50 MB (with full catalog)
- **Docker Image:** ~200 MB
- **Client Binary:** ~6 MB
- **Build Time:** ~2 minutes (server + client)
- **Scrape Time:** ~5 minutes (both sources)

---

## 🎉 Acknowledgments

Built with contributions from the open-source community and powered by:
- **Claude Sonnet 4.5** - Development assistance
- **Rust Community** - Excellent documentation and crates
- **FitGirl & SteamRIP** - Game repack providers
- **Real-Debrid** - Download infrastructure

**Made with ❤️ for home lab enthusiasts**

---

**Version:** 2.1.0
**Last Updated:** February 7, 2026
**Changelog:** See [RELEASE_NOTES.md](RELEASE_NOTES.md)
