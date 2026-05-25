FROM rust:latest AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates/agentdir/Cargo.toml crates/agentdir/Cargo.toml
COPY crates/agentdir-cli/Cargo.toml crates/agentdir-cli/Cargo.toml

RUN mkdir -p crates/agentdir/src crates/agentdir-cli/src && \
    echo "pub fn version() -> &'static str { \"0\" }" > crates/agentdir/src/lib.rs && \
    echo "fn main() {}" > crates/agentdir-cli/src/main.rs && \
    cargo build --workspace 2>/dev/null || true && \
    rm -rf crates/

COPY . .

RUN cargo fmt --check && \
    cargo clippy --workspace -- -D warnings && \
    cargo test --workspace && \
    cargo build --workspace --release

FROM debian:bookworm-slim AS runtime
COPY --from=builder /app/target/release/agentdir /usr/local/bin/agentdir
ENTRYPOINT ["agentdir"]
