# Build stage
FROM rust:1.84-bookworm AS builder

WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies only (cached layer)
RUN cargo build --release && rm -rf src

# Copy actual source code
COPY src ./src

# Build the application
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/weathrs /app/weathrs

# Copy example config (user should mount their own config.toml)
COPY config.example.toml /app/config.example.toml

# Create non-root user
RUN useradd -r -s /bin/false weathrs && chown -R weathrs:weathrs /app
USER weathrs

EXPOSE 3000

ENV RUST_LOG=weathrs=info,tower_http=info

CMD ["./weathrs"]
