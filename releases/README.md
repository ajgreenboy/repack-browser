# Repack Browser - Release Binaries

This directory contains pre-built binaries for distribution.

## Windows Client Agent

### Latest Release

**Filename:** repack-client-windows-x64.exe  
**Platform:** Windows 10/11 (x86_64)  
**Build Type:** Release (optimized)

### Installation

1. Download repack-client-windows-x64.exe
2. Run the executable
3. Configure the config file created at %APPDATA%\RepackClient\config.toml
4. Restart the client

### Building from Source

To build the Windows client from Linux:

```bash
cd client-agent
cargo build --release --target x86_64-pc-windows-gnu
```

The binary will be at: target/x86_64-pc-windows-gnu/release/repack-client.exe

### Requirements

- Windows 10 or later
- .NET Framework (usually pre-installed)
- Internet connection to reach the server

### Antivirus Notes

Some antivirus software may flag the executable as suspicious. Add exclusions as needed.
