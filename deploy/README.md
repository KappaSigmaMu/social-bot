# Production deployment (DigitalOcean droplet)

Single-droplet deployment using Docker Compose and images built by GitHub Actions to GHCR.

```
Push to main → GitHub Actions → ghcr.io/kappasigmamu/social-bot:latest
                                       ↓
                               docker compose pull (on droplet)
```

## Prerequisites

- A DigitalOcean droplet (1 vCPU / 1 GB RAM is enough)
- Ubuntu 24.04 LTS or Debian 12
- A Matrix bot account with access token, invited and joined to the target room
- Outbound HTTPS to the Matrix homeserver and WSS to `RPC_URL`
- GitHub Actions has run at least once on `main` so the image exists in GHCR

## One-time droplet bootstrap

### 1. Create the droplet

Create a Basic droplet (1 vCPU / 1 GB) in a region close to your Matrix homeserver and RPC endpoint. Use Ubuntu 24.04 LTS.

### 2. Install Docker

```bash
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER
```

Log out and back in so the `docker` group applies.

### 3. Configure firewall

The bot is outbound-only — no inbound ports except SSH.

```bash
sudo ufw default deny incoming
sudo ufw allow OpenSSH
sudo ufw enable
```

### 4. Set up the app directory

```bash
sudo mkdir -p /opt/social-bot/data /opt/social-bot/backups
sudo chown -R $USER:$USER /opt/social-bot
cd /opt/social-bot
```

Copy deploy files from your laptop:

```bash
scp deploy/docker-compose.yml deploy/.env.example root@YOUR_DROPLET:/opt/social-bot/
```

If you have an existing override database locally, copy it too:

```bash
scp society_overrides.db root@YOUR_DROPLET:/opt/social-bot/data/society_overrides.db
ssh root@YOUR_DROPLET 'chown 1000:1000 /opt/social-bot/data/society_overrides.db'
```

Create `.env` from the template:

```bash
cp .env.example .env   # after copying deploy/.env.example here
chmod 600 .env
```

Fill in production values:

```env
MATRIX_ROOM=!your-room:matrix.org
MATRIX_TOKEN=...
MATRIX_USER_ID=@kappasigmabot:matrix.org
MATRIX_HOMESERVER=https://matrix.org
RPC_URL=wss://kusama-rpc.polkadot.io/
DB_PATH=/data/society_overrides.db
PREFIX=!
RUST_LOG=info
# X_WEBHOOK_URL=           # Make.com webhook URL; leave unset to disable X
# HEALTHCHECK_ADDR=127.0.0.1:8080
```

Ensure the data directory is writable by the container user (UID 1000):

```bash
chown -R 1000:1000 data
```

### Migrating from `/opt/element-bot`

The old deploy directory stays as an archive. To move:

```bash
sudo mkdir -p /opt/social-bot/data /opt/social-bot/backups
sudo cp /opt/element-bot/.env /opt/social-bot/.env
sudo cp /opt/element-bot/data/society_overrides.db /opt/social-bot/data/
sudo chown -R 1000:1000 /opt/social-bot/data
```

Update the deploy compose files (they now reference `social-bot`), then start as below. The old `element-bot` container can be stopped/removed once the new one is healthy.

### 5. Authenticate to GHCR

The image is published to `ghcr.io/kappasigmamu/social-bot`. Create a GitHub personal access token with `read:packages` scope, then:

```bash
echo $GITHUB_PAT | docker login ghcr.io -u YOUR_GITHUB_USER --password-stdin
```

If the package is private, the token needs `read:packages`. Public packages can be pulled without login once the repo's package visibility is set to public in GitHub.

### 6. Start the bot

Smoke test (sends one Matrix message and exits):

```bash
docker compose pull
docker compose run --rm social-bot --sample
```

If that succeeds, start the daemon:

```bash
docker compose up -d
docker compose logs -f --tail=50
```

Send `!ping` in the Matrix room to confirm the bot is live.

## Healthcheck

`deploy/docker-compose.yml` publishes the healthcheck port and Docker checks it every 30 s via `curl`. Confirm it is reachable on the droplet:

```bash
curl -fsS http://127.0.0.1:8080/health
```

The compose healthcheck also keeps `docker compose ps` informative:

```bash
docker compose ps
```

## Updates

After each merge to `main`, GitHub Actions rebuilds and pushes `:latest`. To deploy from your laptop:

```bash
cargo deploy
```

Add to your local `.env`:

```env
DEPLOY_HOST=root@your-droplet-ip
# DEPLOY_DIR=/opt/social-bot   # optional, this is the default
```

This SSHes into the droplet and runs `docker compose pull && docker compose up -d`.

Or manually on the droplet:

```bash
cd /opt/social-bot
docker compose pull && docker compose up -d
```

SQLite overrides and logs in `./data/` survive restarts and image updates.

## Monitoring

| Check | Command |
|-------|---------|
| Container running / health | `docker compose ps` |
| Recent logs | `docker compose logs -f --tail=100` |
| Functional probe | Send `!ping` in the Matrix room |
| HTTP healthcheck | `curl -fsS http://127.0.0.1:8080/health` |

Watch for repeated `sync failed; retrying` or `RPC connection failed` in logs.

## Backups

SQLite overrides are the only persistent state that matters. Nightly backup via cron:

```bash
crontab -e
```

Add:

```
0 3 * * * cp /opt/social-bot/data/society_overrides.db /opt/social-bot/backups/$(date +\%F).db
```

Bot logs also roll daily into `/opt/social-bot/data/logs/`.

## Troubleshooting

**Permission denied on `/data`**

The container runs as UID 1000. Fix ownership:

```bash
chown -R 1000:1000 /opt/social-bot/data
```

**Image pull fails**

Confirm GitHub Actions completed on `main` and the package exists at `ghcr.io/kappasigmamu/social-bot`. Re-run `docker login ghcr.io` if the PAT expired.

**Bot does not respond**

- Confirm the bot account is joined to `MATRIX_ROOM`.
- Verify `MATRIX_TOKEN` is valid and matches `MATRIX_USER_ID`.
- Check logs: `docker compose logs social-bot`.

**Healthcheck says unhealthy**

- Confirm `HEALTHCHECK_ADDR` is reachable inside the container; the compose file publishes port 8080 and the healthcheck uses `curl` (installed in the runtime image).
- If X posts are failing, check logs for `X webhook post failed` — this never affects Matrix output.

## What not to do

- Do not run two instances against the same room/DB.
- Do not build Rust on the droplet — use the CI-built image.
- Do not commit `.env` or bake secrets into the image.
