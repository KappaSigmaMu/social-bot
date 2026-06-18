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
    let society = Arc::new(Society::new(chain, store));
    let matrix = MatrixClient::new(
        &config.matrix_homeserver,
        config.matrix_token,
        config.matrix_user_id,
    )?;

    let period_matrix = matrix.clone();
    let period_room = config.matrix_room.clone();
    let period_society = society.clone();
    tokio::spawn(async move {
        if let Err(err) = period_matrix
            .announce_period_changes(period_room, period_society)
            .await
        {
            tracing::error!(?err, "period announcer stopped");
        }
    });

    matrix
        .run(&config.matrix_room, &config.prefix, society)
        .await
}
