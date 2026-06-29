# Docker E2E

This harness runs the bot against:

- a Chopsticks fork at `ws://chopsticks:8000`, using the copied Asset Hub config [kusama.yml](/Users/laurogripa/code/kusama/element-bot/tests/e2e/kusama.yml)
- a tiny mock Matrix homeserver
- a Python assertion runner that exercises `!ping`, `!head`, `!period`, `!defender`, `!skeptics`, `!candidates`, `!set_address`, `!me`, and `!unset_address`

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

The Rust `submit_bid` helper is not usable with this copied Asset Hub config; `society.bid` is filtered there and fails with `System::CallFiltered`.
