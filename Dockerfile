# Stage 1: Build Vue frontend
FROM node:22-alpine AS frontend-builder
WORKDIR /app/crates/tdx-web
COPY crates/tdx-web/package.json crates/tdx-web/package-lock.json* ./
RUN npm ci
COPY crates/tdx-web/ ./
RUN npm run build

# Stage 2: Build Rust backend
FROM rust:1.82-alpine AS rust-builder
RUN apk add --no-cache musl-dev sqlite-dev

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/tdx-maintain-core/Cargo.toml crates/tdx-maintain-core/
COPY crates/tdx-maintain-server/Cargo.toml crates/tdx-maintain-server/
RUN mkdir -p crates/tdx-maintain-core/src crates/tdx-maintain-server/src && \
    echo 'fn main() {}' > crates/tdx-maintain-server/src/main.rs && \
    echo '' > crates/tdx-maintain-core/src/lib.rs && \
    cargo build --release 2>/dev/null; true

COPY crates/tdx-maintain-core/src/ crates/tdx-maintain-core/src/
COPY crates/tdx-maintain-server/src/ crates/tdx-maintain-server/src/
COPY config/ config/
COPY migrations/ migrations/
COPY --from=frontend-builder /app/crates/tdx-web/dist crates/tdx-web/dist/
RUN cargo build --release -p tdx-maintain-server

# Stage 3: Runtime
FROM alpine:3.21
RUN apk add --no-cache ca-certificates sqlite-libs tzdata && \
    cp /usr/share/zoneinfo/Asia/Shanghai /etc/localtime && \
    echo "Asia/Shanghai" > /etc/timezone

WORKDIR /app
COPY --from=rust-builder /app/target/release/tdx-maintain-server /app/
COPY config/ /app/config/
COPY migrations/ /app/migrations/

RUN mkdir -p /data/tdx /data/tdx_maintain/backup /data/tdx_maintain/parquet

EXPOSE 8080
ENV RUST_LOG=info
ENV TDX_MAINTAIN_CONFIG=/app/config/default.toml

HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
  CMD wget -qO- http://localhost:8080/api/health || exit 1

CMD ["./tdx-maintain-server"]
