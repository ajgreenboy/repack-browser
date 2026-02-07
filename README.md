# Repack Browser

A **self-hosted game repack browser** for households. Browse 7500+ games from multiple sources (FitGirl, SteamRIP), download directly to your PC, and auto-install with one click.

> **⚠️ For Personal/Household Use Only**
>
> This application is designed for **home networks**. Each household member runs the Windows client on their own PC. Downloads happen locally, not on the server.
>
> **Real-Debrid:** Each user needs their own Real-Debrid account. Sharing one account violates [Real-Debrid's TOS](https://real-debrid.com/terms).

---

## 🎯 How It Works

1. **Server** (Docker) - Hosts the game catalog, web interface, and ONE Real-Debrid account
2. **Windows Client** - Runs on each PC, downloads files to local disk
3. **Web Browser** - Browse games from any device on your network

**Workflow:**
```
Browse website → Click Download → Server converts via Real-Debrid
  ↓
Client polls server → Gets direct download URLs
  ↓
Client downloads to YOUR PC → Auto-extracts → Auto-installs → Reports progress
```

**Key Points:**
- Server admin sets up ONE Real-Debrid account for the household
- All users share the same RD account (allowed by RD for same IP)
- Downloads happen on each user's own PC, not on the server
- Client reports progress back to server for tracking

---

## ✨ Features

### 🎮 Game Catalog
- **7500+ games** from FitGirl Repacks and SteamRIP
- **Advanced search** with genre filtering and sorting
- **Screenshot galleries** for each game
- **Favorites system** to bookmark games
- **Random picker** for discovering new games

### 👥 Multi-User Support
- **Separate accounts** for each household member
- **Personal favorites** and download history per user
- **Session-based authentication** with secure cookies
- **Admin controls** for managing the system

### 📥 Smart Downloads
- **Downloads to YOUR PC** - Not the server!
- **Real-Debrid integration** - Converts magnets to fast direct downloads
- **Progress tracking** - Real-time speed, ETA, and progress bars
- **Auto-extraction** - Handles ZIP and 7Z archives
- **Silent installation** - FitGirl repacks install automatically with no prompts
- **Desktop notifications** - Get notified at every stage

### 🔧 Windows Client Features
- **Local HTTP server** - Browser communicates with your local client
- **Real-Debrid** - Each user uses their own RD account
- **Download manager** - Handles multiple files with resume support
- **Auto-extractor** - Extracts archives to your games folder
- **Silent installer** - Runs FitGirl setups with `/VERYSILENT /LANG=english`
- **Notifications** - Windows popups for download/extract/install status

---

## 🚀 Quick Start

### Step 1: Server Setup (Docker)

**Requirements:** Docker, Docker Compose

```bash
# Clone repository
git clone https://github.com/ajgreenboy/repack-browser.git
cd repack-browser

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
2. (Optional) Add RAWG API key for game metadata
3. Click "Scrape" to populate the database (~5 minutes)

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

[realdebrid]
api_key = "YOUR_REAL_DEBRID_API_KEY"  # Get from https://real-debrid.com/apitoken
enabled = true

[extraction]
output_dir = "C:\\Games"
delete_after_extract = false
```

3. **Restart** the client
4. **Keep it running** - Minimize to system tray

#### Get Real-Debrid API Key
1. Go to https://real-debrid.com/apitoken
2. Copy your API key
3. Paste into `config.toml`

---

## 📖 Usage Guide

### Downloading Games

1. **Browse** the website on your PC (where client is running)
2. **Click** a game to view details
3. **Click** "Download" button
4. **Watch** notifications appear:
   - "Processing Download..." (converting magnet via RD)
   - "Downloading..." (file downloading to your PC)
   - "Download Complete! Extracting..."
   - "Extraction Complete! Installing..."
   - "Installation Complete!"

**That's it!** The game is now installed on your PC.

### If Download Fails

**Error: "Could not connect to Repack Client"**
- Make sure the Windows client is running on your PC
- Client must be on the same PC as your browser

**Error: "Real-Debrid is not configured"**
- Add your RD API key to `config.toml`
- Set `enabled = true`
- Restart the client

---

## 🏗️ Architecture

### Server (Docker Container)
**Responsibilities:**
- Host game catalog (SQLite database)
- Serve web interface
- Handle user authentication
- Track download progress (reported by clients)

**Does NOT:**
- Download files
- Extract archives
- Install games

### Windows Client (Per PC)
**Responsibilities:**
- Run HTTP server on `localhost:9999` for browser commands
- Download files to local disk via Real-Debrid
- Extract archives locally
- Install games silently
- Report progress to server

**Each client:**
- Uses its own Real-Debrid account
- Downloads to its own PC
- Has its own output directory

---

## 🛠️ Configuration

### Server Environment Variables

```bash
# Database
DATABASE_PATH=sqlite:/app/data/games.db?mode=rwc

# Optional: RAWG API for game metadata
RAWG_API_KEY=your_key_here

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
poll_interval_secs = 30

[realdebrid]
api_key = "YOUR_KEY_HERE"  # Required!
enabled = true

[extraction]
output_dir = "C:\\Games"
watch_dir = "C:\\Users\\YourName\\Downloads"
delete_after_extract = false
verify_md5 = true

[monitoring]
report_interval_secs = 2
track_ram_usage = true
```

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

### Real-Debrid
- Each user should have their own RD account
- Don't share accounts (violates RD TOS)
- API keys stored locally in client config

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

### Client Issues

**"Could not connect to Repack Client"**
- Client must be running on same PC as browser
- Check if `localhost:9999` is accessible
- Firewall might be blocking port 9999

**Downloads fail:**
- Verify Real-Debrid API key is correct
- Check RD account is active
- Ensure enough disk space

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
2. **Configure** with your own Real-Debrid account
3. **Create account** on the website
4. **Keep client running** while browsing

### What's Shared
- Game catalog (everyone sees same games)
- Server resources (bandwidth, storage)

### What's Private
- Your downloads (only you see them)
- Your favorites
- Your Real-Debrid account

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

It simply provides a browser interface for publicly available information and connects to your existing Real-Debrid account.

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

### Recent Changes
- ✅ Refactored to client-download architecture
- ✅ Each user downloads to their own PC
- ✅ Real-Debrid integration per client
- ✅ Silent FitGirl installations
- ✅ Desktop notifications
- ✅ Multi-user authentication

### Known Issues
See [ARCHITECTURE_ISSUES.md](ARCHITECTURE_ISSUES.md) for technical details about ongoing refactoring work.

---

**Made with ❤️ for home lab enthusiasts**
