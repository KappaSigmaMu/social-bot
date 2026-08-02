use anyhow::Result;
use social_bot::announce::{run_event_loop, run_period_loop};
use social_bot::chain::{Society, SubxtKusama};
use social_bot::config::Config;
use social_bot::health;
use social_bot::logging;
use social_bot::matrix::MatrixClient;
use social_bot::store::OverrideStore;
use social_bot::x::XWebhook;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> Result<()> {
    let _log_guard = logging::init("social-bot");

    let config = Config::from_env()?;
    let matrix = MatrixClient::new(
        &config.matrix_homeserver,
        config.matrix_token,
        config.matrix_user_id,
    )?;
    let room_id = matrix.resolve_room_id(&config.matrix_room).await?;

    if config.sample_mode {
        matrix
            .send_message(&room_id, "Sample message from social-bot", None)
            .await?;
        return Ok(());
    }

    tokio::spawn(async move {
        if let Err(err) = health::serve(&config.healthcheck_addr).await {
            tracing::error!(?err, "healthcheck server failed");
        }
    });

    let store = Arc::new(Mutex::new(OverrideStore::open(&config.db_path)?));
    let chain = SubxtKusama::connect_with_retry(&config.rpc_url).await;
    let society = Arc::new(Society::new(chain.clone(), store.clone()));
    let x = config.x_webhook_url.clone().map(|url| XWebhook::new(url));

    let period_matrix = matrix.clone();
    let period_room = room_id.clone();
    let period_store = store.clone();
    let period_society = society.clone();
    let period_chain = chain.clone();
    let period_x = x.clone();
    tokio::spawn(async move {
        run_period_loop(
            period_matrix,
            period_room,
            period_store,
            period_society,
            period_chain,
            period_x,
        )
        .await;
    });

    let events_matrix = matrix.clone();
    let events_room = room_id.clone();
    let events_store = store.clone();
    let events_society = society.clone();
    let events_chain = chain.clone();
    let events_x = x.clone();
    tokio::spawn(async move {
        run_event_loop(
            events_matrix,
            events_room,
            events_store,
            events_society,
            events_chain,
            events_x,
        )
        .await;
    });

    matrix.run(&room_id, &config.prefix, society).await
}
