use anyhow::Result;
use element_bot::chain::{Society, SubxtKusama};
use element_bot::config::Config;
use element_bot::logging;
use element_bot::matrix::MatrixClient;
use element_bot::models::SeenSocietyEvents;
use element_bot::store::OverrideStore;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let _log_guard = logging::init("element-bot");

    let config = Config::from_env()?;
    let matrix = MatrixClient::new(
        &config.matrix_homeserver,
        config.matrix_token,
        config.matrix_user_id,
    )?;
    let room_id = matrix.resolve_room_id(&config.matrix_room).await?;

    if config.sample_mode {
        matrix
            .send_message(&room_id, "Sample message from element-bot", None)
            .await?;
        return Ok(());
    }

    let store = OverrideStore::open(&config.db_path)?;
    let chain = SubxtKusama::connect_with_retry(&config.rpc_url).await;
    let seen_society_events = Arc::new(SeenSocietyEvents::new());
    let event_chain = chain.clone();
    let society = Arc::new(Society::new(chain.clone(), store));

    let period_matrix = matrix.clone();
    let period_room = room_id.clone();
    let period_society = society.clone();
    let period_chain = chain.clone();
    tokio::spawn(async move {
        period_matrix
            .announce_period_changes(period_room, period_society, period_chain)
            .await;
    });

    let events_matrix = matrix.clone();
    let events_room = room_id.clone();
    let events_society = society.clone();
    let events_seen = seen_society_events.clone();
    tokio::spawn(async move {
        events_matrix
            .announce_society_events(events_room, events_society, event_chain, events_seen)
            .await;
    });

    matrix.run(&room_id, &config.prefix, society).await
}
