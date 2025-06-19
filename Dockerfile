# Builder
FROM rust:1.80.1-slim AS builder

WORKDIR /app

# Add build dependencies
RUN apt-get update && apt install -yqq \
    git \
    cmake \
    pkg-config \
    libssl-dev \
    ca-certificates

COPY . .

# Add `--no-default-features` if you don't want stats collection
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp /app/target/release/spoticord /app/spoticord

# Runtime
FROM debian:bookworm-slim

# Add runtime dependencies  
RUN apt update && apt install -y ca-certificates

# Copy spoticord binary from builder
COPY --from=builder /app/spoticord /usr/local/bin/spoticord

ENTRYPOINT [ "/usr/local/bin/spoticord" ]
