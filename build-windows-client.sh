#!/bin/bash
set -e

echo "Building Windows client..."
cd client-agent
cargo build --release --target x86_64-pc-windows-gnu

echo "Copying to releases directory..."
mkdir -p ../releases
cp target/x86_64-pc-windows-gnu/release/repack-client.exe ../releases/repack-client-windows-x64.exe

echo "Build complete!"
echo "Binary location: releases/repack-client-windows-x64.exe"
ls -lh ../releases/repack-client-windows-x64.exe
