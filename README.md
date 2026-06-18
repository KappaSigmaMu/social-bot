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
