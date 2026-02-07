#!/bin/bash
gh release create v2.0.0 \
  --title "Repack Browser v2.0.0 - Client-Side Downloads" \
  --notes "## Repack Browser v2.0.0 - Client-Side Downloads

**Major architectural refactor** - Complete rewrite to client-side download model.

### What's New

**Architecture:**
- ONE Real-Debrid account per household - Server admin configures RD
- Client-side downloads - Files download to user's own PC
- Background polling - Client checks server every 30 seconds
- Progress reporting - Real-time updates to server
- Multi-user support - Per-user download tracking

**Features:**
- Server converts magnets via Real-Debrid
- Client downloads, extracts, installs on local PC
- Silent FitGirl installations
- Desktop notifications
- Frontend validates client connection
- Per-user download tracking

### Installation

**Server:**
\`\`\`
docker compose up -d
Set RD_API_KEY environment variable
\`\`\`

**Client:**
1. Download repack-client-windows-x64.exe
2. Run once to generate config
3. Edit config at %APPDATA%\RepackClient\config.toml
4. Set server URL
5. Keep running

### Database

6,678 games total:
- FitGirl Repacks: 6,406
- SteamRIP: 272

### Breaking Changes

- No more localhost:9999
- Server handles all RD operations
- New API endpoints
- Database schema updated

See README.md for complete documentation.

---

For household use only." \
  releases/repack-client-windows-x64.exe
