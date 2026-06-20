# Docker E2E

This harness runs the bot against:

- a Chopsticks Kusama fork at `ws://chopsticks:8000`
- a tiny mock Matrix homeserver
- a Python assertion runner that sends `!head` and waits for the bot response

Run it from the repository root:

```sh
docker compose -f tests/e2e/docker-compose.yml up --build --abort-on-container-exit --exit-code-from e2e
```

Clean up:

```sh
docker compose -f tests/e2e/docker-compose.yml down -v
```

The test needs outbound network access because Chopsticks forks from `wss://kusama-rpc.polkadot.io/` and the first run pulls Docker/npm images.
