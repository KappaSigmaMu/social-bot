use crate::chain::{ChainData, Society};
use crate::commands::handle_command;
use crate::messages::period_message;
use crate::models::CandidatePeriodKind;
use anyhow::{Context, Result};
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use tracing::{error, info};

#[derive(Clone)]
pub struct MatrixClient {
    http: Client,
    homeserver: Url,
    token: String,
    user_id: String,
}

impl MatrixClient {
    pub fn new(homeserver: &str, token: String, user_id: String) -> Result<Self> {
        Ok(Self {
            http: Client::new(),
            homeserver: Url::parse(homeserver).context("parsing Matrix homeserver URL")?,
            token,
            user_id,
        })
    }

    pub async fn send_message(&self, room_id: &str, body: &str) -> Result<()> {
        let txn_id = format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        let url = self.homeserver.join(&format!(
            "/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            url_escape(room_id),
            txn_id
        ))?;
        self.http
            .put(url)
            .bearer_auth(&self.token)
            .json(&json!({
                "msgtype": "m.text",
                "body": body,
            }))
            .send()
            .await?
            .error_for_status()
            .context("sending Matrix message")?;
        Ok(())
    }

    async fn sync(&self, since: Option<&str>) -> Result<SyncResponse> {
        let mut url = self.homeserver.join("/_matrix/client/v3/sync")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("timeout", "30000");
            if let Some(since) = since {
                query.append_pair("since", since);
            }
        }
        Ok(self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()
            .context("Matrix sync failed")?
            .json()
            .await?)
    }

    pub async fn run<C>(&self, room_id: &str, prefix: &str, society: Arc<Society<C>>) -> Result<()>
    where
        C: ChainData + 'static,
    {
        let mut since = None;
        loop {
            match self.sync(since.as_deref()).await {
                Ok(sync) => {
                    if since.is_none() {
                        since = Some(sync.next_batch);
                        continue;
                    }
                    since = Some(sync.next_batch);
                    if let Some(joined_room) = sync.rooms.join.get(room_id) {
                        for event in &joined_room.timeline.events {
                            if event.sender == self.user_id {
                                continue;
                            }
                            let Some(body) = event.content.body.as_deref() else {
                                continue;
                            };
                            match handle_command(
                                society.as_ref(),
                                prefix,
                                &event.sender,
                                body,
                                event.origin_server_ts,
                            )
                            .await
                            {
                                Ok(Some(response)) => {
                                    if let Err(err) = self.send_message(room_id, &response).await {
                                        error!(?err, "failed to send Matrix response");
                                    }
                                }
                                Ok(None) => {}
                                Err(err) => {
                                    error!(?err, "command failed");
                                    let _ = self
                                        .send_message(room_id, &format!("Error: {err:#}"))
                                        .await;
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    error!(?err, "sync failed; retrying");
                    sleep(Duration::from_secs(5)).await;
                }
            }
            info!("Matrix sync cycle complete");
        }
    }

    pub async fn announce_period_changes<C>(
        &self,
        room_id: String,
        society: Arc<Society<C>>,
    ) -> Result<()>
    where
        C: ChainData + 'static,
    {
        let mut last_period: Option<CandidatePeriodKind> = None;
        loop {
            match period_snapshot(society.as_ref()).await {
                Ok((period_kind, message)) => {
                    if let Some(last_period) = last_period {
                        if last_period != period_kind {
                            self.send_message(&room_id, &message).await?;
                        }
                    }
                    last_period = Some(period_kind);
                }
                Err(err) => error!(?err, "failed to poll candidate period"),
            }
            sleep(Duration::from_secs(60)).await;
        }
    }
}

async fn period_snapshot<C>(society: &Society<C>) -> Result<(CandidatePeriodKind, String)>
where
    C: ChainData,
{
    let period = society.get_candidate_period().await?;
    let defender = society.get_defending().await?;
    let candidates = society.get_candidates().await?;
    let head = society.get_head_address().await?;
    let candidate_skeptic = society.get_candidate_skeptic().await?;
    let message = period_message(
        &period,
        &defender,
        &candidates,
        head.as_deref(),
        candidate_skeptic.as_deref(),
        true,
    );
    Ok((period.kind, message))
}

fn url_escape(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct SyncResponse {
    next_batch: String,
    #[serde(default)]
    rooms: Rooms,
}

#[derive(Debug, Default, Deserialize)]
struct Rooms {
    #[serde(default)]
    join: HashMap<String, JoinedRoom>,
}

#[derive(Debug, Deserialize)]
struct JoinedRoom {
    timeline: Timeline,
}

#[derive(Debug, Deserialize)]
struct Timeline {
    #[serde(default)]
    events: Vec<RoomEvent>,
}

#[derive(Debug, Deserialize)]
struct RoomEvent {
    sender: String,
    #[serde(default)]
    origin_server_ts: Option<u64>,
    #[serde(default)]
    content: MessageContent,
}

#[derive(Debug, Default, Deserialize)]
struct MessageContent {
    body: Option<String>,
}
