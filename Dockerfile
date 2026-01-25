# Build stage - Alpine for smaller image with musl
FROM rust:alpine AS builder

# Install build dependencies for native libs
# curl is needed by utoipa-swagger-ui to download Swagger UI assets
RUN apk add --no-cache musl-dev curl

WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies only (cached layer)
RUN cargo build --release && rm -rf src

# Copy actual source code and migrations
COPY src ./src
COPY migrations ./migrations

# Build the application
RUN touch src/main.rs && cargo build --release

# Runtime stage - minimal Alpine image (~6MB)
FROM alpine:3.21

# ca-certificates for HTTPS, wget for healthcheck
RUN apk add --no-cache ca-certificates wget

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/weathrs /app/weathrs

# Copy example config (user should mount their own config.toml)
COPY config.example.toml /app/config.example.toml

# Create non-root user
RUN adduser -D -H -s /sbin/nologin weathrs && chown -R weathrs:weathrs /app
USER weathrs

EXPOSE 3030

ENV RUST_LOG=weathrs=info,tower_http=info

CMD ["./weathrs"]
