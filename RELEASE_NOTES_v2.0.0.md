# Repack Browser v2.0.0 - Client-Side Downloads

**Major architectural refactor** - Complete rewrite to client-side download model for proper household use.

---

## ✨ What's New

### Architecture Changes

- 🏠 **ONE Real-Debrid account per household** - Server admin configures RD, all users share it
- 💻 **Client-side downloads** - Files download to each user's own PC, not the server
- 🔄 **Background polling** - Client checks server every 30 seconds for new downloads
- 📊 **Progress reporting** - Real-time updates from client to server to frontend
- 👥 **Multi-user isolation** - Each user only sees their own downloads

### Features

- ✅ Server converts magnets via Real-Debrid automatically
- ✅ Client downloads, extracts, and installs on local PC
- ✅ Silent FitGirl installations (`/VERYSILENT` flags)
- ✅ Desktop notifications at each stage (download/extract/install)
- ✅ Frontend validates client is connected before allowing downloads
- ✅ Per-user download tracking with `user_id` database column

---

## 📥 Installation

### Server Setup (Docker)

```bash
git clone https://github.com/ajgreenboy/repack-browser.git
cd repack-browser

# Set Real-Debrid API key (REQUIRED)
export RD_API_KEY="your_real_debrid_api_key"

# Start server
docker compose up -d
```

**First time setup:**
1. Access http://your-server:3030
2. Login with admin/admin (change immediately!)
3. Go to Settings → Add Real-Debrid API key
4. Click "Scrape" to populate database

### Client Setup (Windows)

1. **Download** `repack-client-windows-x64.exe` from this release
2. **Run once** to generate default config
3. **Edit** `%APPDATA%\RepackClient\config.toml`:
   ```toml
   [server]
   url = "http://your-server:3030"
   enabled = true
   poll_interval_secs = 30

   [extraction]
   output_dir = "C:\\Games"
   delete_after_extract = false
   ```
4. **Keep running** - Minimize to system tray

---

## 🔧 Configuration

### Server Environment Variables

```bash
# REQUIRED
RD_API_KEY=your_real_debrid_api_key

# Optional
RAWG_API_KEY=your_rawg_key        # For game metadata
DATABASE_PATH=sqlite:/app/data/games.db
PORT=3000
```

### Client Config

Client does **NOT** need Real-Debrid configuration. Server handles all RD operations.

---

## ⚠️ Breaking Changes from v1.x

- **No more localhost:9999** - Client polls server instead of running local HTTP server
- **No per-user RD keys** - Server handles all Real-Debrid operations for entire household
- **Database schema** - Added `user_id` column to downloads table (auto-migrated)
- **New API endpoints** - Frontend uses `/api/downloads/create` instead of local client
- **Download flow** - User clicks download → Server converts via RD → Client polls → Downloads

### Migration Guide

If upgrading from v1.x:

1. Update server: `docker compose down && docker compose build --no-cache && docker compose up -d`
2. Set `RD_API_KEY` environment variable on server
3. Download new Windows client
4. Update client config (remove `[realdebrid]` section)
5. Old downloads will need manual cleanup

---

## 📊 Database

**Total games: 6,678**
- FitGirl Repacks: 6,406
- SteamRIP: 272

---

## 🎯 How It Works

```
User clicks Download → Server converts magnet via Real-Debrid
  ↓
Client polls server → Gets direct download URLs
  ↓
Client downloads to local PC → Reports progress
  ↓
Client extracts → Client installs → Reports completion
```

---

## 📖 Documentation

See [README.md](https://github.com/ajgreenboy/repack-browser/blob/main/README.md) for:
- Complete setup guide
- Troubleshooting
- Configuration reference
- Architecture details
- Security recommendations

---

## 🐛 Known Issues

- Client GUI needs improvement (currently minimal interface)
- Progress tracking may show delays during first migration
- Old server-download code still present (commented out, will be removed in v2.1)

---

## 🔐 Security Notes

- **For home networks only** - Do not expose to internet
- Change default admin password immediately
- Use VPN/Tailscale for remote access
- Real-Debrid API key stored securely on server

---

## ⚖️ Legal

**For educational and household use only.** This tool does not host, distribute, or provide pirated content. It provides a browser interface for publicly available information. Please support game developers by purchasing games legally.

---

**Made with ❤️ for home lab enthusiasts**
