# Architectural Issues - Server vs Client Download Logic

## Overview
The codebase has a **fundamental architectural conflict**: it was originally designed for server-side downloads but has been partially refactored to support client-side downloads. Many server-side components still exist and will cause confusion/conflicts.

---

## 🔴 CRITICAL ISSUES

### 1. **Server Download Manager Still Active**
**Location:** `src/download_manager.rs`, `src/downloader.rs`, `src/extractor.rs`

**Problem:**
- Server has full download management system that downloads TO THE SERVER
- `POST /api/downloads` queues downloads on the server (wrong!)
- `DownloadManager` downloads files to server disk
- `Extractor` extracts files on server
- This conflicts with new client architecture where clients download to their own PCs

**Impact:**
- If someone accidentally uses old endpoint, files download to server
- Server disk fills up with game files
- Downloads tracked in database but files are on server, not client PCs

**Files Affected:**
- `src/download_manager.rs` - Server download queue
- `src/downloader.rs` - Server file downloader
- `src/extractor.rs` - Server file extractor
- `src/realdebrid.rs` - Server Real-Debrid client (clients should use this)
- `src/main.rs` lines 1205-1227 - queue_download endpoint

**Fix Required:**
- Remove or deprecate server download functionality
- Keep only progress tracking/reporting for client downloads
- OR: Make it explicit that server downloads are admin-only feature

---

### 2. **Downloads Table Missing user_id**
**Location:** `src/db.rs` line 205

**Problem:**
```sql
CREATE TABLE IF NOT EXISTS downloads (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    -- NO user_id COLUMN!
    ...
)
```

- Downloads table has NO user_id column
- All downloads are global - everyone sees everyone's downloads
- No way to filter "my downloads" vs roommate's downloads
- client_id was added but that tracks which machine, not which user

**Impact:**
- Roommate A sees roommate B's downloads
- No privacy between users
- Can't show "Your Downloads" page per user
- Download history is shared across all users

**Fix Required:**
- Add `user_id` column to downloads table
- Add foreign key to users table
- Update all queries to filter by user_id
- Update web UI to show only current user's downloads

---

### 3. **System Info Shows Server, Not Clients**
**Location:** `src/main.rs` line 1562, `src/system_info.rs`

**Problem:**
- `GET /api/system-info` returns SERVER system info
- Shows server RAM, disk space, CPU cores
- Frontend displays this in "System Health" section
- Completely meaningless now that downloads are on client PCs

**Current Behavior:**
```javascript
// Frontend shows:
RAM: 64GB (server RAM, not user's PC)
Disk Space: 2TB (server disk, not user's PC)
CPU: 32 cores (server CPU, not user's PC)
```

**Impact:**
- Users see wrong system information
- Pre-installation checks check SERVER requirements, not their PC
- "Do I have enough space?" shows server space, not their space
- Misleading and confusing

**Fix Required:**
- Remove or hide server system info from user UI
- Show client system info from their Windows client instead
- Client should report its system info to server
- Server should store per-client system info in database
- UI should show "Your PC: RAM: 16GB, Disk: 500GB" etc.

---

### 4. **Installation Features Track Server Installs**
**Location:** `src/installation_*.rs` files

**Problem:**
- `installation_assistant.rs` - Installs DLLs on server
- `installation_checker.rs` - Checks server for missing DLLs
- `installation_monitor.rs` - Monitors server installations
- All installation tracking assumes installations happen on server

**Impact:**
- "Installation Assistant" button installs DLLs on server, not user's PC
- Installation logs track server installations
- No tracking of actual client installations

**Fix Required:**
- Installation assistance should be in Windows client, not server
- Client should report installation success/failure to server
- Server should store installation logs per user, sourced from client

---

### 5. **Real-Debrid Client on Server**
**Location:** `src/realdebrid.rs`

**Problem:**
- Server has Real-Debrid client
- Server can convert magnets (uses server's API quota)
- Frontend has "Get RD Links" button that uses server's RD

**Issues:**
- Server RD usage counts against admin's quota
- Users should use their own RD accounts
- Sharing one RD account violates RD Terms of Service

**Current State:**
- Frontend button for RD links still exists
- Can manually trigger server to convert magnets
- Wrong architecture - clients should do this

**Fix Required:**
- Remove server-side RD functionality
- Each user configures their own RD API key in their client
- Server never touches Real-Debrid

---

### 6. **Download Progress Tracking Mismatch**
**Location:** `src/download_manager.rs`, frontend `app.js`

**Problem:**
- Server tracks download progress in database
- Frontend polls server for progress updates
- But files are downloading on CLIENT, not server
- Server has no way to know real progress

**Current Flow:**
```
Client downloads file (knows real progress)
   ↓
Server database shows progress (outdated/wrong)
   ↓
Frontend polls server (shows wrong progress)
```

**Impact:**
- Progress bars show incorrect information
- Speed/ETA calculations are wrong
- Users can't see their actual download progress

**Fix Required:**
- Client should report progress to server periodically
- Add endpoint: `POST /api/downloads/:id/progress`
- Client POSTs every few seconds with real progress
- Server updates database
- Frontend polls server as before (but now has real data)

---

### 7. **Downloads View Shows All Users' Downloads**
**Location:** `frontend/app.js` function `renderDownloads()`

**Problem:**
- Downloads page shows ALL downloads from ALL users
- No filtering by current user
- Privacy issue - roommates see each other's downloads

**Impact:**
- "My Downloads" shows everyone's downloads
- Can see/cancel other users' downloads
- Confusing UX

**Fix Required:**
- Add user_id to downloads table (see issue #2)
- Filter downloads by current logged-in user
- Show "Your Downloads" not "All Downloads"

---

## ⚠️ MEDIUM PRIORITY ISSUES

### 8. **Download File Paths Are Server Paths**
**Problem:**
- file_path column stores server filesystem paths
- Meaningless when files are on client PCs
- Clients have different paths (C:\Games vs D:\Games etc)

**Fix:**
- Store relative paths or just filenames
- Client knows its own output directory from config

---

### 9. **MD5 Validation Endpoint**
**Location:** `src/main.rs` validate_download endpoint

**Problem:**
- Server checks MD5 of files on server disk
- Files are on client disk now

**Fix:**
- Client should validate MD5 locally
- Report validation status to server

---

### 10. **Scan Existing Games**
**Location:** `src/download_manager.rs` scan_existing_games

**Problem:**
- Scans server filesystem for games
- Should scan client filesystems

**Fix:**
- Remove this feature or make it client-side
- Client scans its own folders and reports to server

---

## 📊 SUMMARY OF CONFLICTS

| Feature | Old (Server) | New (Client) | Status |
|---------|--------------|--------------|--------|
| File Download | Server downloads | Client downloads | ❌ Both exist |
| File Storage | Server disk | Client disk | ❌ Confusion |
| Real-Debrid | Server API | Client API | ❌ Both exist |
| System Info | Server specs | Client specs | ❌ Shows server |
| Installation | Server installs | Client installs | ❌ Tracks server |
| Progress | Server tracks | Client reports | ❌ No reporting |
| Downloads View | Global | Per-user | ❌ No user_id |
| DLL Installation | Server | Client | ❌ Wrong machine |

---

## 🔧 RECOMMENDED FIX PRIORITY

### Phase 1 - Critical (breaks core functionality):
1. Add user_id to downloads table
2. Filter downloads by user in API and UI
3. Add progress reporting from client to server
4. Remove/deprecate server download endpoints

### Phase 2 - Important (misleading UX):
5. Replace server system info with client system info
6. Move installation assistance to client
7. Remove server RD functionality

### Phase 3 - Polish:
8. Clean up file path storage
9. Client-side MD5 validation
10. Remove server-side scan feature

---

## 💡 SUGGESTED ARCHITECTURE

### What Server Should Do:
- Store game catalog
- Handle user authentication
- Track download status (reported by clients)
- Store installation logs (reported by clients)
- Serve web UI

### What Server Should NOT Do:
- Download files
- Extract archives
- Install games
- Use Real-Debrid
- Run installers

### What Client Should Do:
- Download files to local disk
- Extract archives locally
- Install games locally
- Use own Real-Debrid account
- Report progress/status to server

### What Client Should NOT Do:
- Store game catalog (fetch from server)
- Handle authentication (server does this)
- Serve web UI (server does this)

---

**Generated:** 2026-02-07
**Codebase Version:** After commits 62a2e21 and 72886b3
