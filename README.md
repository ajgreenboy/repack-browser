# FitGirl Browser

A self-hosted web app for browsing, searching, and managing FitGirl Repacks. Scrapes the FitGirl site for game metadata, images, and magnet links, with optional Real-Debrid integration for direct download links.

## Features

- Scrapes ~3500 games with metadata (genres, developer, screenshots, file sizes)
- Search, genre filtering, sorting (date, title, size)
- Favorites system
- Random game picker
- Screenshot gallery per game
- Real-Debrid integration for unrestricted download links
- RAWG API integration for gap-filling missing images/metadata
- Settings UI for API key management
- SQLite database, persists across restarts

## Quick Start (Docker)

```bash
git clone <repo-url> fitgirl-browser
cd fitgirl-browser
docker compose up -d --build
```

First build takes a few minutes (Rust compilation). Access at `http://localhost:3030`.

Click **Scrape** in the top bar to populate the database. Takes about 5 minutes.

## Configuration

All configuration is done through the web UI under **Settings**, or via environment variables in `docker-compose.yml`:

| Variable | Description | Required |
|---|---|---|
| `RD_API_KEY` | Real-Debrid API key ([get one here](https://real-debrid.com/apitoken)) | No — only for download links |
| `RAWG_API_KEY` | RAWG API key ([get one here](https://rawg.io/apidocs)) | No — only for gap-filling missing images |
| `DOWNLOAD_DIR` | Path for downloaded files | No — defaults to `/app/downloads` |
| `AUTO_EXTRACT` | Auto-extract archives after download | No — defaults to `true` |
| `DELETE_ARCHIVES` | Delete archives after extraction | No — defaults to `false` |

API keys set through the Settings UI are stored in the database and take priority over environment variables.

## Volumes

| Path | Purpose |
|---|---|
| `./data` | SQLite database (persists game data, settings, favorites) |
| `./downloads` | Downloaded files |

## Building from Source (no Docker)

Requires Rust 1.85+.

```bash
cargo build --release
./target/release/fitgirl-browser
```

The binary serves the frontend from a `frontend/` directory relative to itself. Access at `http://localhost:3000`.

## Reverse Proxy

If running behind Nginx, Caddy, Nginx Proxy Manager, etc., point it at port `3030` (or `3000` if running the binary directly).

## Tech Stack

- **Backend:** Rust, Axum, SQLite (sqlx)
- **Frontend:** Vanilla JS, custom CSS
- **Scraping:** WordPress REST API, HTML parsing
- **APIs:** RAWG (game metadata), Real-Debrid (download links)

## License

MIT
