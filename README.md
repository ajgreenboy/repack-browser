# FitGirl Scraper

A comprehensive self-hosted solution for browsing, downloading, and managing game repacks. Scrapes multiple sources (FitGirl Repacks, SteamRIP) with Real-Debrid integration, system health monitoring, and a Windows client agent for distributed extraction.

## Screenshots

### Library View
![Library View](Screenshots/home.png)

### Game Details
![Game Details](Screenshots/gamedlpage.png)

### Download Links
![Download Links](Screenshots/links.png)

## Features

### Core Features
- 🎮 **Multi-source scraping** - ~7500+ games from FitGirl Repacks and SteamRIP
- 🔍 **Advanced search** - Search, genre filtering, sorting (date, title, size)
- ⭐ **Favorites system** - Mark and filter your favorite games
- 🎲 **Random picker** - Discover new games randomly
- 🖼️ **Screenshot galleries** - View game screenshots before downloading
- 🔗 **Real-Debrid integration** - Convert magnet links and DDL to direct downloads
- 🎨 **RAWG API integration** - Auto-fill missing game metadata and images

### Download Management
- 📥 **Queue system** - Manage multiple downloads
- 📊 **Real-time progress** - Live download speed, ETA, and progress bars
- 📦 **Auto-extraction** - Automatically extract .zip and .7z archives
- ✅ **MD5 validation** - Verify file integrity after extraction
- 🔄 **Retry failed downloads** - Automatic retry with error recovery
- 🗑️ **Smart cleanup** - Optional archive deletion after extraction

### System Health & Installation
- 💻 **System monitoring** - Track RAM, disk space, CPU cores, missing DLLs
- ⚠️ **Pre-install checks** - Validate system requirements before installation
- 🛠️ **Installation assistant** - One-click DLL installation and AV exclusions
- 📝 **Installation logs** - Track success/failure with error analysis
- 📈 **Community ratings** - Share installation difficulty and issues
- 🔍 **Failure analysis** - AI-powered recommendations for failed installations

### Windows Client Agent
- 🖥️ **Distributed extraction** - Offload extraction to Windows clients
- 🌐 **Multi-user support** - Track multiple clients on your network
- 📡 **Real-time reporting** - Live extraction progress from each client
- 🤖 **Auto-watch folders** - Automatically extract files dropped in watch folder
- 🆔 **Client tracking** - Unique UUID per client with system info

## Quick Start

### Docker (Recommended)

1. **Clone the repository:**
```bash
git clone https://github.com/ajgreenboy/fitgirl-browser.git fitgirl-scraper
cd fitgirl-scraper
```

2. **Copy example config:**
```bash
cp docker-compose.example.yml docker-compose.yml
# Edit docker-compose.yml with your settings
```

3. **Start the server:**
```bash
docker compose up -d --build
```

4. **Access the web UI:**
Open `http://localhost:3030` in your browser.

5. **Initial setup:**
   - Click **Settings** and add your API keys (optional but recommended)
   - Click **Scrape** to populate the database (~5 minutes)

### Building from Source

Requires Rust 1.85+ and SQLite.

```bash
cargo build --release
./target/release/fitgirl-browser
```

Access at `http://localhost:3000`.

## Windows Client Agent

For distributed extraction and multi-user setups, deploy the Windows client agent on each PC.

### Quick Setup

1. **Download** `client-agent/fitgirl-client.exe` from this repository
2. **Run** the executable - it creates a config file automatically
3. **Configure** `%APPDATA%\FitGirlClient\config.toml`:
   ```toml
   [server]
   url = "http://your-server-ip:3030"
   enabled = true
   
   [extraction]
   output_dir = "C:\Games"
   watch_dir = "C:\Users\YourName\Downloads"
   ```
4. **Restart** the client

See [client-agent/README.md](client-agent/README.md) for full documentation.

## Configuration

All configuration can be done through the web UI under **Settings**, or via environment variables.

See [.env.example](.env.example) for all available options.

## Tech Stack

- **Backend:** Rust (Axum framework, SQLite via sqlx)
- **Frontend:** Vanilla JavaScript, custom CSS
- **Scraping:** WordPress REST API, HTML parsing
- **APIs:** Real-Debrid, RAWG
- **Client Agent:** Rust (Windows-specific)

## License

MIT

## Disclaimer

This tool is for educational purposes. Please support game developers by purchasing games legally.
