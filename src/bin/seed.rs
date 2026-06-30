use anyhow::{Context, Result};
use element_bot::config::Config;
use element_bot::overrides_import::{HistoryMessage, replay_override_history};
use element_bot::room_archive::load_history_messages;
use element_bot::store::OverrideStore;
use std::env;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let mut input_dir = None;
    let mut db_path = None;
    let mut dry_run = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input_dir = Some(args.next().context("--input requires a value")?),
            "--db-path" => db_path = Some(args.next().context("--db-path requires a value")?),
            "--dry-run" => dry_run = true,
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    let config = Config::from_env()?;
    let input_dir = input_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./room-export"));
    let db_path = db_path.unwrap_or(config.db_path);

    tracing::info!(input = %input_dir.display(), "loading export");
    let messages = load_history_messages(&input_dir)?;
    tracing::info!(messages = messages.len(), "loaded export messages");

    let command_messages = count_command_messages(&messages, &config.prefix);
    tracing::info!(
        command_messages,
        prefix = %config.prefix,
        "found override-related commands"
    );

    if dry_run {
        let store = OverrideStore::open(":memory:")?;
        let stats =
            replay_override_history(&store, &config.prefix, &config.matrix_user_id, &messages)?;
        print_stats(&stats, true);
        for (address, matrix_handle) in store.list_overrides()? {
            tracing::info!(%address, %matrix_handle, "would seed override");
        }
        return Ok(());
    }

    let store = OverrideStore::open(&db_path)?;
    let stats = replay_override_history(&store, &config.prefix, &config.matrix_user_id, &messages)?;
    print_stats(&stats, false);
    tracing::info!(db_path = %db_path, "seeded override database");
    for (address, matrix_handle) in store.list_overrides()? {
        tracing::info!(%address, %matrix_handle, "seeded override");
    }
    Ok(())
}

fn count_command_messages(messages: &[HistoryMessage], prefix: &str) -> u64 {
    messages
        .iter()
        .filter(|message| {
            let Some(body) = message.body.strip_prefix(prefix) else {
                return false;
            };
            matches!(
                body.split_whitespace().next(),
                Some("set_address") | Some("unset_address")
            )
        })
        .count() as u64
}

fn print_stats(stats: &element_bot::overrides_import::ImportStats, dry_run: bool) {
    tracing::info!(
        messages_seen = stats.messages_seen,
        set_commands = stats.set_commands,
        unset_commands = stats.unset_commands,
        invalid_commands = stats.invalid_commands,
        final_overrides = stats.final_overrides,
        dry_run,
        "seed complete"
    );
}

fn print_usage() {
    eprintln!(
        "Seed society_overrides.db from a cargo export directory.\n\n\
         Usage: cargo seed [--input <export-dir>] [--db-path <path>] [--dry-run]\n\n\
         Reads events.jsonl from the export produced by cargo export and replays\n\
         every !set_address / !unset_address command in chronological order.\n\n\
         Defaults come from .env:\n\
           PREFIX, MATRIX_USER_ID, DB_PATH\n\
           --input defaults to ./room-export\n\n\
         Example:\n\
           cargo export --output ./exports/kappasigmamulounge\n\
           cargo seed --input ./exports/kappasigmamulounge --dry-run\n\
           cargo seed --input ./exports/kappasigmamulounge --db-path ./society_overrides.db"
    );
}
