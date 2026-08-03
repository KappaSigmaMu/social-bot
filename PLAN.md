# PLAN: social-bot

Transform `element-bot` into `social-bot`: a Kusama Society monitor with two
outputs — Element (Matrix), restructured around weekly round threads, and X
(Twitter) — plus an expanded event surface and a healthcheck endpoint.

## Phases at a glance

| Phase | Deliverable |
|---|---|
| 0 | Rename project to `social-bot` |
| 1 | SQLite persistence (seen events, round state) |
| 2 | Element threading: 1 main message per round, everything else in-thread |
| 3 | Expanded Society event monitoring (all meaningful events) |
| 4 | X output via Buffer (strictly free) |
| 5 | Healthcheck HTTP endpoint |

Each phase is independently testable and committable. Implement in order.

---

## Phase 0 — Rename `element-bot` → `social-bot`

Project-facing rename only. The Matrix account (now `@kappasigmabot`) and the
`society_*` domain names (Society pallet terminology) stay.

### Code edits

| File | Change |
|---|---|
| `Cargo.toml` | `name = "social-bot"` (L2), `default-run = "social-bot"` (L5). Cargo.lock regenerates itself — do not hand-edit. |
| All `use element_bot::…` | → `use social_bot::…` in `src/main.rs` (6×), `src/bin/seed.rs` (5×), `src/bin/export.rs` (4×), `src/bin/cargo-dev.rs`, `src/bin/cargo-chopsticks.rs` |
| `Dockerfile` | L13, L15 `--bin social-bot`; L24 COPY path → `/usr/local/bin/social-bot`; L29 `ENTRYPOINT ["social-bot"]` |
| `src/main.rs` | L12 `logging::init("social-bot")`; L24 sample text → `"Sample message from social-bot"` |
| `src/bin/cargo-dev.rs` | L33 `["run", "--bin", "social-bot", "--", "--dev"]`; L38, L46 messages |
| `src/bin/cargo-chopsticks.rs` | L94 temp filename prefix → `social-bot-chopsticks-` |
| `src/bin/deploy.rs` | L5 `DEFAULT_DEPLOY_DIR = "/opt/social-bot"`; update test at L54–55 |
| `deploy/docker-compose.yml` | service `social-bot:`; `image: ghcr.io/kappasigmamu/social-bot:latest` |
| `deploy/README.md` | all `/opt/element-bot` → `/opt/social-bot`; image name; add a "Migrating from /opt/element-bot" note (new dir, copy `data/` + `.env`, `chown 1000:1000`) |
| `README.md` | title `# social-bot`; L57 `RUST_LOG` example → `social_bot=debug`; fix doc-link path containing repo dir |
| `tests/e2e/README.md` | doc-link path containing repo dir |
| `.env.example`, `deploy/.env.example` | `# DEPLOY_DIR=/opt/social-bot` |

Not renamed: `.github/workflows/docker.yml` (uses `${{ github.repository }}` —
follows the GitHub rename automatically); `tests/e2e/docker-compose.yml`
(service is already the generic `bot`); `@societybot:e2e.local` e2e mock IDs;
cargo aliases in `.cargo/config.toml` (they reference bin *file* names, unchanged).

While here: production default Matrix IDs still say `@societybot:matrix.org`
(`src/config.rs` L56 + test L109, `src/bin/seed.rs` L10 + L115,
`src/overrides_import.rs` L251 test const, `.env.example` L4). The real account
is now `@kappasigmabot` — update production defaults/examples to
`@kappasigmabot:matrix.org`. Leave e2e-local values untouched.

### Manual steps (document in README/deploy README, not code)

1. Rename GitHub repo `KappaSigmaMu/element-bot` → `KappaSigmaMu/social-bot`
   (Settings → rename; GitHub redirects old git URLs).
2. Rename local clone dir; `git remote set-url origin git@github.com:KappaSigmaMu/social-bot.git`.
3. After the first CI build: new GHCR package `ghcr.io/kappasigmamu/social-bot`
   appears — set it public / link it to the repo (old `element-bot` package remains as archive).
4. Droplet migration: create `/opt/social-bot`, copy `data/` and `.env` from
   `/opt/element-bot`, deploy with new compose.

---

## Phase 1 — SQLite persistence

Restarts currently lose (a) seen bid/unbid events (duplicate re-announcements)
and (b) any future thread-root linkage. Fix both in `src/store.rs` (same
existing DB file, `CREATE TABLE IF NOT EXISTS` — no migration tooling):

```sql
CREATE TABLE IF NOT EXISTS seen_society_events(
  block_hash TEXT NOT NULL,
  event_index INTEGER NOT NULL,
  kind TEXT NOT NULL,
  PRIMARY KEY (block_hash, event_index, kind)
);
CREATE TABLE IF NOT EXISTS round_state(
  id INTEGER PRIMARY KEY CHECK (id = 1),
  root_event_id TEXT NOT NULL,
  started_relay_block INTEGER NOT NULL
);
```

- Methods: `is_society_event_seen`, `mark_society_event_seen`,
  `round_root()` / `set_round_root(root_event_id, relay_block)` (single-row upsert),
  `clear_round_root()`.
- `announce_society_events` checks/marks through the store instead of the
  in-memory `SeenSocietyEvents` (remove `SeenSocietyEvents` or keep it as a
  write-through cache — implementer's choice; prefer deletion, less code).
- Unit tests with `tempfile` (existing dev-dep), matching current store tests.

---

## Phase 2 — Element threading

**New behavior:** the main room gets exactly ONE message per rotation round
(the round/challenge-start message). Everything else until the next round —
bids, votes, claim start, suspensions, … — goes into that message's thread.

### Matrix client changes (`src/matrix.rs`)

- `send_message` returns the new `event_id` (parse `{"event_id": …}` from the
  PUT response — currently discarded).
- `message_payload` (L469) gains an optional thread parameter. When present,
  set the relation per the Matrix thread spec:
  ```json
  "m.relates_to": {
    "rel_type": "m.thread",
    "event_id": "<root_event_id>",
    "is_falling_back": true,
    "m.in_reply_to": { "event_id": "<root_event_id>" }
  }
  ```
  (fallback `in_reply_to` points at the root — simple and spec-compliant.)

### Announce flow (new `src/announce.rs`; loops moved out of `matrix.rs`)

- Move `announce_period_changes`, `announce_society_events`, `period_snapshot`
  here unchanged in detection logic.
- On **Voting round start**: send `period_message(..., new_period: true)` to the
  MAIN channel → store returned `event_id` via `set_round_root`.
- Every other announcement: if `round_root()` exists → send as thread message;
  else (mid-round restart, empty DB) → main channel fallback, log a warning.
- **Claim start** ("voting end/claim start") becomes a thread message with a new
  dedicated composer `claim_started_message(period, candidates)` that includes
  per-candidate tallies (`{approvals}/{rejections}` from the existing
  `candidates_raw` data) — replaces the current "New voting period started"
  wording that wrongly prefixes claim announcements (`messages.rs` L35–37).
- Root-creation send failure → no event_id → thread messages fall back to main
  channel until the next round; log the error.
- `matrix.run` command loop and `!`-command replies are unchanged (main channel).

---

## Phase 3 — Expanded Society event monitoring

pallet-society 23.0.0 emits 17 events; only `Bid`/`Unbid` are handled today.
Decoding is dynamic string-matching in `parse_society_event` (`src/chain.rs`
L572–607), so new events are mechanical: match arm + model variant + composer.
No metadata codegen needed.

### Approved routing map

| Event | Element main | Element thread | X |
|---|---|---|---|
| Round start (Voting; local, computed) | ✅ root msg | — | ✅ |
| Claim start (Voting end; local, computed) | — | ✅ | ✅ |
| `Bid` / `Unbid` | — | ✅ | ✅ |
| `Vouch(candidate_id, offer, vouching)` | — | ✅ | ✅ |
| `Unvouch(candidate)` | — | ✅ | — |
| `AutoUnbid(candidate)` | — | ✅ | — |
| `Inducted(primary, candidates[])` | — | ✅ | ✅ |
| `Challenged(member)` | — | ✅ | ✅ |
| `CandidateSuspended` / `MemberSuspended` | — | ✅ | — |
| `SuspendedMemberJudgement(who, judged)` | — | ✅ | — |
| `Elevated(member, rank)` | — | ✅ | — |
| `Vote(candidate, voter, approve)` | — | ✅ (individual) | — |
| `DefenderVote(voter, approve)` | — | ✅ (individual) | — |
| `Founded`/`Unfounded`/`NewParams`/`Deposit` | ignored (rare/genesis/config noise; revisit later) | | |

### Implementation notes

- `src/models.rs`: extend `SocietyEventKind` + `SocietyEvent` with the new
  variants and fields; dedupe keying is automatic via the kind in `SocietyEventId`.
- Multi-account extraction caveat: `find_account_ids` returns accounts in
  document order. `Vouch`: first = candidate, second = voucher. `Inducted`:
  first = primary (new head), remaining = inducted candidates. `Vote`: first =
  candidate, second = voter. Encode this order-dependence in the match arms and
  cover it with tests using realistic JSON shapes (field names per pallet
  source: `candidate_id`, `offer`, `vouching`, `primary`, `candidates`,
  `member`, `voter`, `vote` (bool), `judged`, `rank`).
- **Identity display names** (user requirement, esp. for votes): extend the
  existing `Identity.IdentityOf` query (`chain.rs` L556 `identity_matrix_handle`
  reads only the `riot` field) with `identity_display_name(address)` reading the
  `display` field. Composers render `Display Name (@handle)` → `Display Name` →
  address, in that priority. Add a small in-memory memo map in the announce loop
  (a voter votes on many candidates — avoid refetching the same identity).
- Vote composers:
  - `Vote`: `{voter} voted {approve|reject} on candidate {candidate}`
  - `DefenderVote`: `{voter} voted {approve|reject} on defender {defender}`
    (defender address from the existing `Defending` storage, already fetched in
    `period_snapshot`)
- All new messages are thread-only, plain and one-line; X composers only for
  the X subset (Phase 4).
- Unit-test every new `parse_society_event` arm and composer.

### Pre-existing issue to flag (do not fix in this PR unless trivial)

`Society.NextIntakeAt` (`chain.rs` L548) does not exist in pallet-society
23.0.0 — dead query. Verify it fails gracefully (returns `None`) and note it in
README; removal/fix is a separate change.

---

## Phase 4 — X output via Buffer

**Confirmed:** Buffer's free plan includes posting to connected social channels.
Buffer runs the integration under its own X API agreement: **zero X API cost,
zero X developer account, ToS-compliant**. Fallback if this ever changes:
Pabbly Connect free tier (webhook pattern); last resort: official pay-per-use
API (~$0.25/mo at our volume) — do not implement fallbacks now.

### Manual setup (do first; ~15 min)

1. Pick/create the bot's X account and connect it to Buffer as a channel.
2. Buffer → **Settings → API** → generate an API key.
3. Use the Buffer API (or API Explorer) to find the channel ID for the X
   account: query `channels(input: { organizationId: "…" }) { id name service }`
   and pick the `twitter` service id.
4. `X_BUFFER_API_KEY=<key>` and `X_BUFFER_CHANNEL_ID=<id>` in `.env`.

### Code

- `src/config.rs`: optional `x_buffer_api_key` / `x_buffer_channel_id`
  (`Option<String>`, missing/empty → `None`; X disabled, behavior identical to
  today) plus `x_buffer_url` (defaults to `https://api.buffer.com`). Tests.
- `src/x.rs`: `XBuffer { http: reqwest::Client, url, api_key, channel_id }` with
  `payload(channel_id, text) -> serde_json::Value` (GraphQL `createPost`
  mutation, `mode: shareNow`, pure, tested) and `post(&self, text) ->
  Result<()>` (Err on non-2xx or GraphQL `MutationError`, with message). Same
  reqwest style as `matrix.rs`. No new crates.
- `src/announce.rs`: `x: Option<XBuffer>` and a `dispatch(matrix_text, thread,
  x_text: Option<&str>)` helper — sends the Matrix part (thread or main per
  Phase 2) and, if configured and `x_text` is `Some`, posts to Buffer.
  **Per-output error isolation**: an X failure logs `error!` and never blocks or
  delays Matrix, and vice versa.
- `src/messages.rs` — X composers for the X subset only. Plain text, no
  markdown, **no URLs**, ≤ 280 chars, block number included for uniqueness
  (X rejects identical repeat posts):
  - Round start: `New Kusama Society voting period started (block {b}). {n} candidate(s), ~{duration} to vote.`
  - Claim start: `Kusama Society claim period started (block {b}). Candidates with a clear majority may claim membership.`
  - Bid: `New Kusama Society bid (block {b}): {display} — {amount} KSM`
  - Unbid: `Kusama Society bid withdrawn (block {b}): {display}`
  - Vouch: `New Kusama Society vouch (block {b}): {voucher} vouches for {candidate} — {amount} KSM`
  - Inducted: `Kusama Society inducted {n} new member(s) (block {b}). New head: {primary_display}` (truncate lists to fit 280)
  - Challenged: `Kusama Society member challenged (block {b}): {display} — defender vote is on.`
  KSM formatting matches existing `KSM_DIVISOR` convention. Test content + length.

---

## Phase 5 — Healthcheck endpoint

- `src/health.rs`: hand-rolled HTTP responder on `tokio::net::TcpListener`
  (no new deps — matches repo style). `GET /health` → `200` with
  `{"status":"ok","uptime_secs":N}`; any other path → `404`. Bound from
  `HEALTHCHECK_ADDR` (default `127.0.0.1:8080`); spawned as a task in `main`.
- `Dockerfile`: `EXPOSE 8080`.
- `deploy/docker-compose.yml`: map the port and add a healthcheck. The slim
  runtime image has no curl/wget — use bash's `/dev/tcp`:
  ```yaml
  healthcheck:
    test: ["CMD", "bash", "-c", "exec 3<>/dev/tcp/localhost/8080 && printf 'GET /health HTTP/1.0\r\n\r\n' >&3 && grep -q '200 OK' <&3"]
    interval: 30s
    timeout: 5s
    retries: 3
  ```
  (Verify `bash` presence in `debian:bookworm-slim` during implementation; if
  absent, install `curl` in the runtime stage instead and use it.)
- Unit test: bind an ephemeral port, assert `/health` → 200 + JSON body and
  `/nope` → 404.

---

## Testing

- **Unit** (inline `#[cfg(test)]` per repo convention): config parsing;
  store tables; payload/composers (incl. ≤280 chars); new parse arms with
  realistic JSON; thread payload JSON shape; health handler.
- **E2E** (`tests/e2e/`, docker): extend to assert —
  1. round start → exactly one main-channel message; subsequent events → thread
     messages referencing its `event_id` (`m.thread` relation);
   2. a tiny mock Buffer API receiver (extend `matrix_mock.py` or add
      `x_mock.py`) records X posts for the X subset only (assert NO post for
      votes);
  3. `curl localhost:8080/health` from the e2e runner → 200.
  Update `tests/e2e/README.md`.
- **Gates per phase:** `cargo fmt --check`, `cargo check`, `cargo test`.

## Docs

- `README.md`: rename; new features (threading model, expanded events, X
  output, healthcheck); config table += `X_BUFFER_API_KEY`, `X_BUFFER_CHANNEL_ID`,
  `HEALTHCHECK_ADDR`; Buffer setup summary; note votes go to thread only;
  `NextIntakeAt` caveat.
- `deploy/README.md`: migration note, new env vars, healthcheck, GHCR rename.
- `.env.example` / `deploy/.env.example`: new vars (commented).

## Acceptance criteria

1. Project builds/runs as `social-bot`; CI publishes `ghcr.io/kappasigmamu/social-bot`.
2. Element: one main-channel message per round; all other announcements in its
   thread, including claim start and every individual vote with identity
   display names; commands unchanged.
3. X: the approved subset posts via the Buffer API (`shareNow`) immediately; X
   off by default (no `X_BUFFER_API_KEY` / `X_BUFFER_CHANNEL_ID`) with zero
   behavior change.
4. Restarts do not duplicate event announcements and do not lose the round thread.
5. `curl $HOST:8080/health` → `200 {"status":"ok",…}`.
6. `cargo fmt --check`, `cargo check`, `cargo test` green.
