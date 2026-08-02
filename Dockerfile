FROM rust:1.90-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
COPY Cargo.toml Cargo.lock ./
RUN cargo chef cook --release --recipe-path recipe.json --bin social-bot
COPY src ./src
RUN cargo build --release --locked --bin social-bot

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 1000 -g root bot

COPY --from=builder /app/target/release/social-bot /usr/local/bin/social-bot

EXPOSE 8080

WORKDIR /data
USER bot

ENTRYPOINT ["social-bot"]
