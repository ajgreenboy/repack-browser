# Repack Client Agent

Windows desktop application for managing game downloads and installations.

## Features

### ✅ **System Health Checking**
- Checks local Windows system (RAM, disk, CPU)
- Detects missing DLLs (unarc.dll, ISDone.dll)
- Monitors Windows Defender status
- Auto-installs missing dependencies

### ✅ **Archive Extraction**
- Extracts .zip, .rar, .7z files
- Real-time progress tracking
- Speed and ETA calculation
- Automatic MD5 verification
- RAM usage monitoring

### ✅ **Client Identification**
- Unique client ID (persistent)
- Optional friendly name
- Multi-user support on same network
- Server tracking of downloads per client

### ✅ **Server Integration**
- Syncs download queue from central server
- Reports extraction progress in real-time
- Can operate offline (local mode)
- Auto-reconnect when server available

## Architecture

```
┌─────────────────────┐
│  FitGirl Server     │
│  (homelab:3000)     │
└──────────┬──────────┘
           │ WebSocket/HTTP
           │
    ┌──────┴───────┐
    │              │
┌───▼────┐    ┌───▼────┐
│ Client │    │ Client │
│   #1   │    │   #2   │
│ (Win)  │    │ (Win)  │
└────────┘    └────────┘
```

## Configuration

Client config stored in `%APPDATA%\RepackClient\config.toml`:

```toml
[client]
id = "550e8400-e29b-41d4-a716-446655440000"
name = "Living Room PC"

[server]
url = "http://homelab:3000"
enabled = true

[extraction]
output_dir = "C:\\Games"
delete_after_extract = false
verify_md5 = true

[monitoring]
report_interval_secs = 2
track_ram_usage = true
```

## Usage

### **First Run**
1. Launch `repack-client.exe`
2. System tray icon appears
3. Right-click → Settings
4. Configure server URL and output directory
5. Agent auto-registers with server

### **Download & Extract**
1. Queue downloads from web UI
2. Client automatically fetches queue
3. Archives extract to configured directory
4. Progress shown in system tray
5. Server shows real-time metrics

### **System Health**
- Right-click tray icon → "Check System"
- Shows local Windows system status
- Offers to fix issues (install DLLs, etc.)

## Building

```bash
cd client-agent
cargo build --release
```

Output: `target/release/repack-client.exe` (~5MB)

## API Endpoints (Client → Server)

```
POST /api/clients/register          - Register new client
GET  /api/clients/:id/queue         - Get download queue
POST /api/clients/:id/progress      - Report extraction progress
POST /api/clients/:id/system-info   - Report system status
```

## Troubleshooting

**Client won't connect to server:**
- Check server URL in config
- Ensure server is running
- Check firewall settings

**Extraction fails:**
- Verify archive isn't corrupted
- Check disk space
- Run as Administrator for DLL operations

**System tray icon missing:**
- Check Windows notification area settings
- Restart explorer.exe

## License

Same as parent project
