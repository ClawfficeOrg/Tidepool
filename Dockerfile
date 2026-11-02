# Build stage
FROM rust:1.88-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
# The workspace declares every member; copy each one's manifest and source so
# Cargo can resolve the workspace, then build only the server package.
COPY crates ./crates
COPY sdk/rust ./sdk/rust
COPY cli ./cli
COPY tidepool-console ./tidepool-console
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release -p tidepool && rm -rf src

# Build actual application
COPY src ./src
# `sqlx::migrate!` embeds this directory at compile time.
COPY migrations ./migrations
RUN touch src/main.rs && cargo build --release -p tidepool

# Runtime stage
FROM debian:bookworm-slim AS runtime

WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/tidepool /app/tidepool

EXPOSE 8080

ENV RUST_LOG=info

ENTRYPOINT ["/app/tidepool"]
