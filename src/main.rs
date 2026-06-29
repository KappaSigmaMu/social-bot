use anyhow::Result;
use element_bot::chain::{Society, SubxtKusama};
use element_bot::config::Config;
use element_bot::matrix::MatrixClient;
use element_bot::store::OverrideStore;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env()?;
    let store = OverrideStore::open(&config.db_path)?;
    let chain = SubxtKusama::connect(&config.rpc_url).await?;
    let event_chain = chain.clone();
    let society = Arc::new(Society::new(chain, store));
    let matrix = MatrixClient::new(
        &config.matrix_homeserver,
        config.matrix_token,
        config.matrix_user_id,
    )?;
    let room_id = matrix.resolve_room_id(&config.matrix_room).await?;

    let period_matrix = matrix.clone();
    let period_room = room_id.clone();
    let period_society = society.clone();
    tokio::spawn(async move {
        if let Err(err) = period_matrix
            .announce_period_changes(period_room, period_society)
            .await
        {
            tracing::error!(?err, "period announcer stopped");
        }
    });

    let events_matrix = matrix.clone();
    let events_room = room_id.clone();
    let events_society = society.clone();
    tokio::spawn(async move {
        if let Err(err) = events_matrix
            .announce_society_events(events_room, events_society, event_chain)
            .await
        {
            tracing::error!(?err, "society event announcer stopped");
        }
    });

    matrix.run(&room_id, &config.prefix, society).await
}
