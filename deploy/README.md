# Production deployment (DigitalOcean droplet)

Single-droplet deployment using Docker Compose and images built by GitHub Actions to GHCR.

```
Push to main → GitHub Actions → ghcr.io/kappasigmamu/element-bot:latest
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
sudo mkdir -p /opt/element-bot/data /opt/element-bot/backups
sudo chown -R $USER:$USER /opt/element-bot
cd /opt/element-bot
```

Copy deploy files from your laptop:

```bash
scp deploy/docker-compose.yml deploy/.env.example root@YOUR_DROPLET:/opt/element-bot/
```

If you have an existing override database locally, copy it too:

```bash
scp society_overrides.db root@YOUR_DROPLET:/opt/element-bot/data/society_overrides.db
ssh root@YOUR_DROPLET 'chown 1000:1000 /opt/element-bot/data/society_overrides.db'
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
MATRIX_USER_ID=@yourbot:matrix.org
MATRIX_HOMESERVER=https://matrix.org
RPC_URL=wss://kusama-rpc.polkadot.io/
DB_PATH=/data/society_overrides.db
PREFIX=!
RUST_LOG=info
```

Ensure the data directory is writable by the container user (UID 1000):

```bash
chown -R 1000:1000 data
```

### 5. Authenticate to GHCR

The image is published to `ghcr.io/kappasigmamu/element-bot`. Create a GitHub personal access token with `read:packages` scope, then:

```bash
echo $GITHUB_PAT | docker login ghcr.io -u YOUR_GITHUB_USER --password-stdin
```

If the package is private, the token needs `read:packages`. Public packages can be pulled without login once the repo's package visibility is set to public in GitHub.

### 6. Start the bot

Smoke test (sends one Matrix message and exits):

```bash
docker compose pull
docker compose run --rm element-bot --sample
```

If that succeeds, start the daemon:

```bash
docker compose up -d
docker compose logs -f --tail=50
```

Send `!ping` in the Matrix room to confirm the bot is live.

## Updates

After each merge to `main`, GitHub Actions rebuilds and pushes `:latest`. To deploy from your laptop:

```bash
cargo deploy
```

Add to your local `.env`:

```env
DEPLOY_HOST=root@your-droplet-ip
# DEPLOY_DIR=/opt/element-bot   # optional, this is the default
```

This SSHes into the droplet and runs `docker compose pull && docker compose up -d`.

Or manually on the droplet:

```bash
cd /opt/element-bot
docker compose pull && docker compose up -d
```

SQLite overrides and logs in `./data/` survive restarts and image updates.

## Monitoring

| Check | Command |
|-------|---------|
| Container running | `docker compose ps` |
| Recent logs | `docker compose logs -f --tail=100` |
| Functional probe | Send `!ping` in the Matrix room |

Watch for repeated `sync failed; retrying` or `RPC connection failed` in logs.

## Backups

SQLite overrides are the only persistent state that matters. Nightly backup via cron:

```bash
crontab -e
```

Add:

```
0 3 * * * cp /opt/element-bot/data/society_overrides.db /opt/element-bot/backups/$(date +\%F).db
```

Bot logs also roll daily into `/opt/element-bot/data/logs/`.

## Troubleshooting

**Permission denied on `/data`**

The container runs as UID 1000. Fix ownership:

```bash
chown -R 1000:1000 /opt/element-bot/data
```

**Image pull fails**

Confirm GitHub Actions completed on `main` and the package exists at `ghcr.io/kappasigmamu/element-bot`. Re-run `docker login ghcr.io` if the PAT expired.

**Bot does not respond**

- Confirm the bot account is joined to `MATRIX_ROOM`.
- Verify `MATRIX_TOKEN` is valid and matches `MATRIX_USER_ID`.
- Check logs: `docker compose logs element-bot`.

**Duplicate bid/unbid announcements after restart**

Expected — in-memory dedupe resets on restart. This is a known limitation, not a deployment issue.

## What not to do

- Do not run two instances against the same room/DB.
- Do not build Rust on the droplet — use the CI-built image.
- Do not commit `.env` or bake secrets into the image.
