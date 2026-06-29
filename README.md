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
It also polls Society bids every 60 seconds and announces newly observed bids to the configured Matrix room.

## Configuration

Copy `.env.example` to `.env` and fill in the Matrix bot settings.

```sh
cargo run
```

To point the bot at local Chopsticks without editing `.env`, pass:

```sh
cargo run -- --dev
```

To send one sample Matrix message to the configured room and exit:

```sh
cargo run -- --sample
```

Required:

- `MATRIX_ROOM`: Matrix room ID (for example `!room:matrix.org`) or room alias (for example `#room:matrix.org`).
- `MATRIX_TOKEN`: access token for the bot account.

Optional:

- `MATRIX_HOMESERVER`: defaults to `https://matrix.org`.
- `MATRIX_USER_ID`: defaults to `@societybot:matrix.org`, but you should set this explicitly to the bot account that owns `MATRIX_TOKEN`.
- `RPC_URL`: defaults to `wss://kusama-rpc.polkadot.io/`.
- `DB_PATH`: defaults to `./society_overrides.db`.
- `PREFIX`: defaults to `!`.
- `RUST_LOG`: tracing filter, for example `info` or `element_bot=debug`.

## Manual Testing

Use this when you want to validate the bot against a real Matrix homeserver and live Kusama data instead of the Docker e2e harness.

Prerequisites:

- Rust installed locally.
- A Matrix bot account.
- An access token for that bot account.
- The bot account's Matrix user ID, such as `@societybot:matrix.org`.
- A dedicated Matrix test room.
- The bot account invited to that room and already joined.
- Outbound network access to the Matrix homeserver and `RPC_URL`.
- A writable path for `DB_PATH`.

Setup:

1. Copy `.env.example` to `.env`.
2. Create a dedicated Matrix room for testing. A private room is safer because some commands write persistent address overrides.
3. Invite the bot account to the room and confirm it has joined.
4. Fill in at least these values in `.env`:
   - `MATRIX_ROOM` with the room ID or alias for the test room
   - `MATRIX_TOKEN` with the bot access token
   - `MATRIX_USER_ID` with the exact user ID for that token
   - `MATRIX_HOMESERVER` if you are not using `https://matrix.org`
5. Start the bot:

```sh
cargo run
```

6. Confirm startup succeeds and the bot begins syncing instead of failing with Matrix auth, room resolution, or RPC connection errors.

Suggested manual checks from another Matrix account in the test room:

- `!ping`
- `!head`
- `!period`
- `!defender`
- `!skeptics`
- `!candidates`
- `!info <known-kusama-address>`
- `!set_address <known-member-address>`
- `!me`
- `!unset_address`

Expected results:

- The bot replies in the configured room.
- Read-only commands return live Kusama-backed data.
- `!set_address` stores the Matrix handle override and `!me` uses it.
- After restarting the bot, the override is still present if you kept the same `DB_PATH`.

Notes:

- `!info` and `!set_address` are easiest to verify with a known Kusama Society member address.
- Period transition announcements are only emitted when the live candidate period changes while the bot is running, so that behavior is not practical to fully verify on demand.
- Manual-test output changes over time because it depends on live chain state.

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

The default checked-in Chopsticks config is [tests/e2e/kusama.yml](/Users/laurogripa/code/kusama/element-bot/tests/e2e/kusama.yml), copied from `../kappasigmamu.github.io/config/kusama.yml`. That file targets Asset Hub.

To run that exact copied config locally:

```sh
make chopsticks
```

The `submit_bid` helper does not work against this copied Asset Hub config; `society.bid` fails there with `System::CallFiltered`.
