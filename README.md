# Repack Browser

A **self-hosted game repack browser** for households. Browse 6,600+ games from multiple sources (FitGirl, SteamRIP), download directly to your PC, and auto-install with one click.

> **⚠️ For Personal/Household Use Only**
>
> This application is designed for **home networks**. Server admin sets up ONE Real-Debrid account. All household members share this account (allowed by RD for same IP address). Downloads happen on each user's own PC, not the server.

---

## 🎯 How It Works

1. **Server** (Docker) - Hosts game catalog, web interface, and ONE Real-Debrid account
2. **Windows Client** - Runs on each PC, downloads files to local disk
3. **Web Browser** - Browse games from any device on your network

**Workflow:**
```
User clicks Download → Server converts magnet via Real-Debrid
  ↓
Client polls server (every 30s) → Gets direct download URLs
  ↓
Client downloads to YOUR PC → Auto-extracts → Auto-installs
  ↓
Client reports progress back to server
```

**Key Points:**
- Server admin sets up **ONE** Real-Debrid account for the household
- All users share the same RD account (allowed by RD for same IP)
- Downloads happen on each user's **own PC**, not on the server
- Client reports progress back to server for tracking

---

## ✨ Features

### 🎮 Game Catalog
- **6,600+ games** from FitGirl Repacks and SteamRIP
- **Advanced search** with genre filtering and sorting
- **Screenshot galleries** for each game
- **Favorites system** to bookmark games
- **Random picker** for discovering new games

### 👥 Multi-User Support
- **Separate accounts** for each household member
- **Personal favorites** and download history per user
- **Session-based authentication** with secure cookies
- **Admin controls** for managing the system
- **Client status tracking** - See which clients are online

### 📥 Smart Downloads
- **Downloads to YOUR PC** - Not the server!
- **Real-Debrid integration** - Server converts magnets to fast direct downloads
- **Background polling** - Client checks for new downloads every 30 seconds
- **Progress tracking** - Real-time speed, ETA, and progress bars
- **Auto-extraction** - Handles ZIP and 7Z archives
- **Silent installation** - FitGirl repacks install automatically with no prompts
- **Desktop notifications** - Get notified at every stage

### 🔧 Windows Client Features
- **Background polling** - Checks server for new downloads
- **Download manager** - Handles multiple files with resume support
- **Auto-extractor** - Extracts archives to your games folder
- **Silent installer** - Runs FitGirl setups with `/VERYSILENT /LANG=english`
- **Notifications** - Windows popups for download/extract/install status
- **Progress reporting** - Sends real-time updates back to server

---

## 🚀 Quick Start

### Step 1: Server Setup (Docker)

**Requirements:** Docker, Docker Compose

```bash
# Clone repository
git clone https://github.com/ajgreenboy/repack-browser.git
cd repack-browser

# Set Real-Debrid API key (REQUIRED)
export RD_API_KEY="your_real_debrid_api_key"

# Start server
docker compose up -d
```

**Access:** `http://your-server-ip:3030`

**First login:**
- Username: `admin`
- Password: `admin`
- ⚠️ **Change immediately!**

**Initial setup:**
1. Log in and go to Settings
2. **REQUIRED:** Add Real-Debrid API key (get from https://real-debrid.com/apitoken)
3. (Optional) Add RAWG API key for game metadata
4. Click "Scrape" to populate the database (~5 minutes)

**Real-Debrid Setup (Required):**
- Server admin gets ONE Real-Debrid account for the household
- Go to https://real-debrid.com/apitoken and copy your API key
- Add it to server settings or set `RD_API_KEY` environment variable
- All household members will share this account

---

### Step 2: Windows Client Setup

#### Download Client
Get the latest Windows client from [Releases](https://github.com/ajgreenboy/repack-browser/releases)

Or build from source:
```bash
cd client-agent
cargo build --release --target x86_64-pc-windows-gnu
```

#### Configure Client

1. **Run** `repack-client.exe` once to generate config
2. **Edit** `%APPDATA%\RepackClient\config.toml`:

```toml
[server]
url = "http://your-server-ip:3030"
enabled = true
poll_interval_secs = 30

[extraction]
output_dir = "C:\\Games"
delete_after_extract = false
```

3. **Restart** the client
4. **Keep it running** - Minimize to system tray

**Note:** Client does NOT need Real-Debrid configuration. Server handles all RD operations.

---

## 📖 Usage Guide

### Downloading Games

1. **Browse** the website (must have client running on same PC)
2. **Click** a game to view details
3. **Click** "Download" button
   - Frontend checks if your client is connected
   - If offline, you'll see an error message
4. **Watch** notifications appear:
   - "Download queued..." (server converting magnet)
   - "Downloading..." (file downloading to your PC)
   - "Download Complete! Extracting..."
   - "Extraction Complete! Installing..."
   - "Installation Complete!"

**That's it!** The game is now installed on your PC.

### If Download Fails

**Error: "Could not connect to Repack Client"**
- Client must be running on your PC
- Check if client is polling server (should see activity every 30 seconds)

**Error: "Real-Debrid is not configured"**
- Server admin needs to configure RD API key on the server
- Add RD_API_KEY environment variable or configure in server settings
- Check server logs to verify RD is working

---

## 🏗️ Architecture

### Server (Docker Container)
**Responsibilities:**
- Host game catalog (SQLite database)
- Serve web interface
- Handle user authentication
- **Convert magnets via Real-Debrid** (server has ONE RD account)
- Track download progress (reported by clients)

**Does NOT:**
- Download files
- Extract archives
- Install games

### Windows Client (Per PC)
**Responsibilities:**
- Poll server for pending downloads (every 30 seconds)
- Download files to local disk using direct URLs from server
- Extract archives locally
- Install games silently
- Report progress back to server

**Each client:**
- Downloads to its own PC (not the server)
- Has its own output directory
- Links to user account for tracking downloads

---

## 🛠️ Configuration

### Server Environment Variables

```bash
# Database
DATABASE_PATH=sqlite:/app/data/games.db?mode=rwc

# REQUIRED: Real-Debrid API key (one for entire household)
RD_API_KEY=your_real_debrid_key_here

# Optional: RAWG API for game metadata
RAWG_API_KEY=your_rawg_key_here

# Port (default: 3000)
PORT=3000
```

### Client Configuration

Full `config.toml` reference:

```toml
[client]
id = "auto-generated-uuid"
name = "Your-PC-Name"

[server]
url = "http://homelab:3030"
enabled = true
poll_interval_secs = 30  # Check for new downloads every 30 seconds

[extraction]
output_dir = "C:\\Games"
watch_dir = "C:\\Users\\YourName\\Downloads"
delete_after_extract = false
verify_md5 = true

[monitoring]
report_interval_secs = 2
track_ram_usage = true
```

**Note:** Real-Debrid configuration is NOT needed in client config. Server handles all RD operations.

---

## 🔐 Security

### For Home Networks
- ✅ Run behind your router/firewall
- ✅ Don't expose port 3030 to internet
- ✅ Use VPN/Tailscale for remote access
- ✅ Change default admin password

### Multi-User
- Each user has separate account
- Passwords are bcrypt hashed
- Session cookies are HTTP-only
- 30-day session expiry
- Per-user download tracking with user_id

### Real-Debrid
- Server admin sets up ONE RD account for the household
- All users on same IP address can share one RD account (per RD TOS)
- API key stored securely on server (not on clients)
- Never expose RD_API_KEY environment variable publicly

---

## 📁 File Structure

```
repack-browser/
├── src/                    # Server source (Rust)
├── frontend/               # Web UI (HTML/JS/CSS)
├── client-agent/           # Windows client source (Rust)
│   └── src/
├── releases/               # Pre-built Windows client
│   └── repack-client-windows-x64.exe
├── data/                   # Database (gitignored)
├── docker-compose.yml      # Docker setup
└── Dockerfile
```

---

## 🐛 Troubleshooting

### Server Issues

**Container won't start:**
```bash
docker compose logs fitgirl-browser
```

**Database errors:**
```bash
# Reset database
rm -rf data/
docker compose restart
# Re-scrape games
```

**Real-Debrid not working:**
- Check RD_API_KEY is set: `docker compose config | grep RD_API_KEY`
- Verify API key is valid: https://real-debrid.com/apitoken
- Check server logs: `docker compose logs | grep -i "real-debrid"`

### Client Issues

**"No client registered" or "Client is offline"**
- Client must be running on your PC
- Check if client is polling server (should see periodic activity)
- Verify `server.url` in config points to correct server address
- Check firewall isn't blocking client

**Downloads fail:**
- Verify server has valid Real-Debrid API key configured
- Check server logs for RD errors
- Ensure client has enough disk space
- Check client is polling server successfully

**Extraction fails:**
- Check write permissions on output directory
- Verify archive isn't corrupted
- Check disk space

**Installation doesn't start:**
- Client looks for `setup.exe` in extracted folder
- Some repacks use different installer names
- Check client logs for details

---

## 🏠 For Roommates/Household

### Setup for Each Person

1. **Install client** on your PC
2. **Configure** server URL in client config
3. **Create account** on the website
4. **Keep client running** in background

### What's Shared
- Game catalog (everyone sees same games)
- Server resources (bandwidth, storage)
- **Real-Debrid account** (one account for entire household)

### What's Private
- Your downloads (only you see your own)
- Your favorites
- Your download/install history

---

## 🔄 Updating

### Update Server
```bash
cd repack-browser
git pull
docker compose down
docker compose build --no-cache
docker compose up -d
```

### Update Client
1. Download latest from [Releases](https://github.com/ajgreenboy/repack-browser/releases)
2. Replace old `repack-client.exe`
3. Restart client

---

## 💾 Backup

**Important data:**
- `data/games.db` - Game catalog and user data
- `config.toml` - Client configuration

**Backup script:**
```bash
# Server
cp data/games.db data/games.db.backup

# Client
copy %APPDATA%\RepackClient\config.toml config.toml.backup
```

---

## 🤝 Contributing

This project is for personal/household use. If you want to contribute:

1. Fork the repository
2. Create a feature branch
3. Test thoroughly
4. Submit a pull request

---

## ⚖️ Legal

### Disclaimer
This tool is for **educational purposes**. Please support game developers by **purchasing games legally**.

This application does not:
- Host any copyrighted content
- Distribute game files
- Provide pirated material

It simply provides a browser interface for publicly available information. Server admin connects their Real-Debrid account for download functionality.

### Credits
- Game data from [FitGirl Repacks](https://fitgirl-repacks.site) and [SteamRIP](https://steamrip.com)
- Metadata from [RAWG.io](https://rawg.io)
- Downloads via [Real-Debrid](https://real-debrid.com)

### License
MIT License - See LICENSE file

---

## 🐙 GitHub

**Repository:** https://github.com/ajgreenboy/repack-browser
**Issues:** https://github.com/ajgreenboy/repack-browser/issues
**Releases:** https://github.com/ajgreenboy/repack-browser/releases

---

## 📝 Version

**Current Version:** 2.0.0
**Release Date:** February 2026
**Architecture:** Client-side downloads (v2.0 refactor)

### Recent Changes (v2.0)
- ✅ Refactored to client-download architecture
- ✅ Server uses ONE Real-Debrid account for entire household
- ✅ Each user downloads to their own PC (not server)
- ✅ Client polls server for pending downloads
- ✅ Real-time progress reporting from clients
- ✅ Silent FitGirl installations with desktop notifications
- ✅ Multi-user authentication with per-user download tracking
- ✅ Frontend validates client connection before allowing downloads

### Known Issues
See [GitHub Issues](https://github.com/ajgreenboy/repack-browser/issues) for current bugs and feature requests.

---

**Made with ❤️ for home lab enthusiasts**
