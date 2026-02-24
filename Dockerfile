# ── Stage 1: Builder ──────────────────────────────────────────────
FROM rust:slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release

# ── Stage 2: Runtime ─────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 1000 kube-devops \
    && useradd -u 1000 -g kube-devops -s /bin/false kube-devops

COPY --from=builder /app/target/release/kube-devops /usr/local/bin/kube-devops

USER 1000

EXPOSE 8080 9090 8443

ENTRYPOINT ["kube-devops"]
