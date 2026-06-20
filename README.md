# element-bot

Rust rewrite of `s3krit/society.py`, a Matrix/Element bot for Kusama Society status.

## Behavior

The bot supports the original command set:

- `!ping`
- `!defender`
- `!info <address>`
- `!candidates [address]`
- `!head`
- `!set_address <address>`
- `!unset_address`
- `!me`
- `!period`
- `!skeptics`
- `!skeptic`

It also polls the Kusama candidate period every 60 seconds and announces period transitions to the configured Matrix room.

## Configuration

Copy `.env.example` to `.env` and fill in the Matrix room/token values.

```sh
cargo run
```

Required:

- `MATRIX_ROOM`: Matrix room ID or alias accepted by the homeserver API.
- `MATRIX_TOKEN`: access token for the bot account.

Optional:

- `MATRIX_HOMESERVER`: defaults to `https://matrix.org`.
- `MATRIX_USER_ID`: defaults to `@societybot:matrix.org`.
- `RPC_URL`: defaults to `wss://kusama-rpc.polkadot.io/`.
- `DB_PATH`: defaults to `./society_overrides.db`.
- `PREFIX`: defaults to `!`.
- `RUST_LOG`: tracing filter, for example `info` or `element_bot=debug`.

## Development

```sh
cargo fmt --check
cargo check
cargo test
```

## Coverage

Rust does not have a standard-library coverage reporter. Use `cargo-llvm-cov`, which wraps Rust/LLVM source-based coverage:

```sh
cargo install cargo-llvm-cov
./scripts/coverage.sh
```

Reports are written to:

- `target/coverage/html/index.html`
- `target/coverage/lcov.info`

## Docker E2E

The Docker e2e harness starts Chopsticks, a mock Matrix homeserver, the bot, and an assertion runner. It exercises Matrix command handling, live Kusama state reads through Chopsticks, and SQLite address overrides:

```sh
docker compose -f tests/e2e/docker-compose.yml up --build --abort-on-container-exit --exit-code-from e2e
```
