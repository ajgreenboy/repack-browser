$ErrorActionPreference = "Stop"

Write-Host "Building Windows client with sanitized temp directory..."

# Use C:\temp for build artifacts to avoid network drive issues
$env:CARGO_TARGET_DIR = "C:\temp\cargo-target-repack"
New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR | Out-Null

Set-Location "Z:\fitgirl-scraper\client-agent"

# Clean and build
cargo clean
cargo build --release --target x86_64-pc-windows-gnu

# Copy to releases
$releaseDir = "Z:\fitgirl-scraper\releases"
New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null

$sourcePath = Join-Path $env:CARGO_TARGET_DIR "x86_64-pc-windows-gnu\release\repack-client.exe"
$destPath = Join-Path $releaseDir "repack-client-windows-x64.exe"

Copy-Item $sourcePath $destPath -Force

Write-Host "Build complete!"
Write-Host "Binary location: $destPath"
Get-Item $destPath | Select-Object Name, Length, LastWriteTime
