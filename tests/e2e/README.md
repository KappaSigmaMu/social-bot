# Docker E2E

This harness runs the bot against:

- a Chopsticks fork at `ws://chopsticks:8000`, using the copied Asset Hub config [kusama.yml](kusama.yml)
- a tiny mock Matrix homeserver
- a tiny mock X (Twitter) Buffer API receiver
- a Python assertion runner that exercises `!ping`, `!head`, `!period`, `!defender`, `!skeptics`, `!candidates`, `!set_address`, `!me`, and `!unset_address`, plus the `GET /health` HTTP endpoint and the X Buffer wiring

Run it from the repository root:

```sh
docker compose -f tests/e2e/docker-compose.yml up --build --abort-on-container-exit --exit-code-from e2e
```

To run only the seeded Chopsticks fork without the rest of the e2e stack:

```sh
make chopsticks
```

Clean up:

```sh
docker compose -f tests/e2e/docker-compose.yml down -v
```

The test needs outbound network access because Chopsticks forks from `wss://kusama-rpc.polkadot.io/` and the first run pulls Docker/npm images.

## What is (and is not) covered

The e2e stack uses the Asset Hub Kusama fork. The Society pallet there has day-length
rounds and `society.bid` is call-filtered, so no on-chain Society events and no round
transitions happen during the test window. The runner therefore asserts:

- every Matrix command works as before,
- `GET /health` returns `200 {"status":"ok",...}`,
- the X mock Buffer API records zero posts (the wiring works, but nothing is posted).

The announcement/threading flow — round start as the single main-channel message, every
later event in its thread, and the X posts for the approved subset — is exercised
locally against a shortened-round custom runtime:

```sh
git submodule update --init --recursive
cargo chopsticks --custom   # builds the custom WASM on first run (takes minutes)
cargo dev --custom
```

Then, from another terminal, submit bids so the bot announces them:

```sh
cargo society:bid
cargo society:unbid
```

The custom Kusama runtime shortens Society rounds to a handful of blocks, so round
transitions (and their thread/root messages) happen within seconds.
