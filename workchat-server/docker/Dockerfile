# syntax=docker/dockerfile:1
# Build context is the repository root:  docker build -f docker/Dockerfile -t workchat-server .
# Developers and tests only: installs pull the published image (see deploy/docker-compose.yml).

# ---- admin panel (static files) ----
FROM node:22-alpine AS admin
WORKDIR /w/admin-web
COPY admin-web/package.json admin-web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY admin-web/ ./
COPY schema/ /w/schema/
RUN npm run build

# ---- server binary ----
FROM rust:1-slim-trixie AS build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /w
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# Workspace members that are not built here still need their manifests.
COPY tests/e2e/ tests/e2e/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/w/target \
    cargo build --release -p workchat-server --bin workchat-server \
 && cp target/release/workchat-server /workchat-server

# ---- runtime ----
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tini && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /data workchat && mkdir -p /data /srv/admin && chown workchat /data
COPY --from=build /workchat-server /usr/local/bin/workchat-server
COPY --from=admin /w/admin-web/dist /srv/admin
ENV WORKCHAT_BIND=0.0.0.0:8080 WORKCHAT_DATA_DIR=/data WORKCHAT_ADMIN_DIR=/srv/admin
USER workchat
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s CMD ["/usr/local/bin/workchat-server", "healthcheck"]
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/workchat-server"]
