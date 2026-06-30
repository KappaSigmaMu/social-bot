FROM rust:1.90-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 1000 -g root bot

COPY --from=builder /app/target/release/element-bot /usr/local/bin/element-bot

WORKDIR /data
USER bot

ENTRYPOINT ["element-bot"]
