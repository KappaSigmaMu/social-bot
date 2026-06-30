use anyhow::{Context, Result};
use element_bot::config::Config;
use element_bot::matrix::MatrixClient;
use element_bot::room_archive::{ExportOptions, export_room_history, retry_failed_media};
use std::env;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

const DEFAULT_ROOM: &str = "#kappasigmamulounge:parity.io";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let mut room = None;
    let mut homeserver = None;
    let mut output_dir = None;
    let mut links_only = false;
    let mut include_all_events = false;
    let mut retry_failed = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--room" => room = Some(args.next().context("--room requires a value")?),
            "--homeserver" => {
                homeserver = Some(args.next().context("--homeserver requires a value")?);
            }
            "--output" => output_dir = Some(args.next().context("--output requires a value")?),
            "--links-only" => links_only = true,
            "--all-events" => include_all_events = true,
            "--retry-failed" => retry_failed = true,
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    let config = Config::from_env()?;
    let room = room.unwrap_or_else(|| DEFAULT_ROOM.to_owned());
    let homeserver = homeserver.unwrap_or(config.matrix_homeserver);
    let output_dir = output_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./room-export"));

    let matrix = MatrixClient::new(&homeserver, config.matrix_token, config.matrix_user_id)?;

    let stats = if retry_failed {
        tracing::info!(output = %output_dir.display(), "retrying failed media");
        retry_failed_media(&matrix, &output_dir).await?
    } else {
        let room_id = matrix.resolve_room_id(&room).await?;
        tracing::info!(room = %room, room_id = %room_id, output = %output_dir.display(), "starting room export");
        export_room_history(
            &matrix,
            &room_id,
            &room,
            ExportOptions {
                output_dir,
                download_media: !links_only,
                include_all_events,
            },
        )
        .await?
    };

    tracing::info!(
        events = stats.events,
        media_links = stats.media_links,
        media_downloaded = stats.media_downloaded,
        media_failed = stats.media_failed,
        "export finished"
    );
    Ok(())
}

fn print_usage() {
    eprintln!(
        "Export full Matrix room history to JSONL plus media files.\n\n\
         Usage: cargo export [--room <alias-or-id>] [--homeserver <url>] \\\n\
                [--output <dir>] [--links-only] [--all-events] [--retry-failed]\n\n\
         Output:\n\
           events.jsonl   one JSON object per event, oldest first\n\
           metadata.json  export summary\n\
           media/         downloaded attachments when media download is enabled\n\n\
         Use --retry-failed to re-download only missing media from an existing export.\n\
         It reads events.jsonl, skips files already on disk, and updates media_path entries.\n\n\
         Defaults:\n\
           room: #kappasigmamulounge:parity.io\n\
           output: ./room-export\n\
           MATRIX_TOKEN and MATRIX_HOMESERVER from .env\n\n\
         Example:\n\
           cargo export --output ./exports/kappasigmamulounge"
    );
}
