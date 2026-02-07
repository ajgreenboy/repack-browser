# ── Build stage ──
FROM rust:1.85-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

# Pin crates that require unreleased rustc versions
RUN cargo update time@0.3.47 --precise 0.3.36

# Build release binary
RUN cargo build --release

# ── Runtime stage ──
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends     ca-certificates     libssl3     sqlite3     curl     p7zip-full     && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/repack-browser /app/repack-browser

# Copy frontend assets
COPY frontend/ /app/frontend/

# Database will be stored in a volume
RUN mkdir -p /app/data

ENV DATABASE_PATH=sqlite:/app/data/games.db?mode=rwc
ENV DOWNLOAD_DIR=/app/downloads

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3     CMD curl -f http://localhost:3000/api/health || exit 1

CMD ["/app/repack-browser"]
