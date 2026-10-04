# syntax=docker/dockerfile:1
FROM rust:1.98-bookworm AS builder
RUN rustup component add rustfmt clippy
RUN apt-get update && apt-get install -y --no-install-recommends poppler-utils \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace/src/backend_api
COPY src/frontend/ /workspace/src/frontend/
COPY src/backend_api/ /workspace/src/backend_api/
COPY tests/backend_api/ /workspace/tests/backend_api/
# BuildKit keeps dependency compilation and registry downloads across source edits.
# Export the binary outside the cache mount so the final image can copy it.
RUN --mount=type=cache,id=helpyourself-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=helpyourself-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=helpyourself-cargo-target,target=/workspace/build/backend_api/target \
    cargo fmt --all --check \
    && cargo clippy --locked --all-targets -- -D warnings \
    && cargo test --locked --all-targets \
    && cargo build --locked --release --bin helpyourself \
    && mkdir -p /workspace/dist/backend_api \
    && cp /workspace/build/backend_api/target/release/helpyourself /workspace/dist/backend_api/helpyourself

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata gosu poppler-utils \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/dist/backend_api/helpyourself /app/helpyourself
COPY docker/entrypoint.sh /app/entrypoint.sh
RUN chmod 755 /app/entrypoint.sh
ENV PUID=1000 PGID=1000 UMASK=077 TZ=UTC
EXPOSE 8080
ENTRYPOINT ["/app/entrypoint.sh", "/app/helpyourself"]
CMD ["--data-dir", "/data", "serve"]
