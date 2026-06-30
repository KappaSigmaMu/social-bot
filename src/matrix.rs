use crate::chain::{ChainData, Society, SubxtKusama};
use crate::commands::handle_command;
use crate::messages::{new_bid_message, period_message, unbid_message};
use crate::models::{Bid, CandidatePeriodKind, SeenSocietyEvents, SocietyEvent};
use crate::overrides_import::HistoryMessage;
use anyhow::{Context, Result};
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomTarget {
    RoomId(String),
    RoomAlias(String),
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

    pub async fn send_message(
        &self,
        room_id: &str,
        body: &str,
        in_reply_to: Option<&str>,
    ) -> Result<()> {
        let txn_id = format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        let url = self.send_message_url(room_id, &txn_id)?;
        self.http
            .put(url)
            .bearer_auth(&self.token)
            .json(&message_payload(body, in_reply_to))
            .send()
            .await?
            .error_for_status()
            .context("sending Matrix message")?;
        info!(
            room_id,
            message = strip_markdown(body),
            in_reply_to,
            "sent Matrix message"
        );
        Ok(())
    }

    async fn sync(&self, since: Option<&str>) -> Result<SyncResponse> {
        let url = self.sync_url(since)?;
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

    pub async fn resolve_room_id(&self, room: &str) -> Result<String> {
        match parse_room_target(room)? {
            RoomTarget::RoomId(room_id) => Ok(room_id),
            RoomTarget::RoomAlias(alias) => self.lookup_room_alias(&alias).await,
        }
    }

    fn send_message_url(&self, room_id: &str, txn_id: &str) -> Result<Url> {
        Ok(self.homeserver.join(&format!(
            "/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            url_escape(room_id),
            txn_id
        ))?)
    }

    fn sync_url(&self, since: Option<&str>) -> Result<Url> {
        let mut url = self.homeserver.join("/_matrix/client/v3/sync")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("timeout", "30000");
            if let Some(since) = since {
                query.append_pair("since", since);
            }
        }
        Ok(url)
    }

    fn room_alias_url(&self, alias: &str) -> Result<Url> {
        Ok(self.homeserver.join(&format!(
            "/_matrix/client/v3/directory/room/{}",
            url_escape(alias)
        ))?)
    }

    async fn lookup_room_alias(&self, alias: &str) -> Result<String> {
        let url = self.room_alias_url(alias)?;
        let response: RoomAliasResponse = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("resolving Matrix room alias {alias}"))?
            .json()
            .await
            .with_context(|| format!("decoding Matrix room alias response for {alias}"))?;
        Ok(response.room_id)
    }

    pub async fn fetch_all_room_events(
        &self,
        room_id: &str,
        event_types: Option<Vec<String>>,
    ) -> Result<Vec<RoomTimelineEvent>> {
        let mut events = Vec::new();
        let mut from: Option<String> = None;
        let mut pages = 0_u64;
        let filter = match event_types {
            Some(types) if !types.is_empty() => Some(serde_json::to_string(
                &serde_json::json!({ "types": types }),
            )?),
            _ => None,
        };

        loop {
            pages += 1;
            let page = self
                .fetch_room_messages(room_id, from.as_deref(), 250, "b", filter.as_deref())
                .await?;
            let page_events = page.chunk.len();
            events.extend(page.chunk.into_iter().map(RoomTimelineEvent::from));
            info!(
                pages,
                page_events,
                total_events = events.len(),
                "fetched Matrix history page"
            );

            let Some(next_from) = page.end else {
                break;
            };
            if from.as_deref() == Some(next_from.as_str()) {
                break;
            }
            from = Some(next_from);
        }

        events.sort_by_key(|event| event.origin_server_ts);
        Ok(events)
    }

    pub async fn fetch_all_room_messages(&self, room_id: &str) -> Result<Vec<HistoryMessage>> {
        Ok(self
            .fetch_all_room_events(room_id, Some(vec!["m.room.message".to_owned()]))
            .await?
            .into_iter()
            .filter_map(|event| {
                let body = event
                    .content
                    .get("body")
                    .and_then(serde_json::Value::as_str)?;
                Some(HistoryMessage {
                    sender: event.sender,
                    body: body.to_owned(),
                    origin_server_ts: event.origin_server_ts,
                })
            })
            .collect())
    }

    pub async fn download_mxc(&self, mxc_url: &str, destination: &Path) -> Result<()> {
        let url = mxc_http_url(&self.homeserver, mxc_url)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()
            .context("downloading Matrix media")?;
        let bytes = response.bytes().await.context("reading Matrix media")?;
        tokio::fs::write(destination, bytes)
            .await
            .with_context(|| format!("writing {}", destination.display()))?;
        Ok(())
    }

    pub fn homeserver(&self) -> &Url {
        &self.homeserver
    }

    async fn fetch_room_messages(
        &self,
        room_id: &str,
        from: Option<&str>,
        limit: u32,
        direction: &str,
        filter: Option<&str>,
    ) -> Result<MessagesResponse> {
        let url = self.messages_url(room_id, from, limit, direction, filter)?;
        Ok(self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("fetching Matrix history for room {room_id}"))?
            .json()
            .await?)
    }

    fn messages_url(
        &self,
        room_id: &str,
        from: Option<&str>,
        limit: u32,
        direction: &str,
        filter: Option<&str>,
    ) -> Result<Url> {
        let mut url = self.homeserver.join(&format!(
            "/_matrix/client/v3/rooms/{}/messages",
            url_escape(room_id)
        ))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("dir", direction);
            query.append_pair("limit", &limit.to_string());
            if let Some(filter) = filter {
                query.append_pair("filter", filter);
            }
            if let Some(from) = from {
                query.append_pair("from", from);
            }
        }
        Ok(url)
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
                            info!(
                                room_id,
                                sender = %event.sender,
                                message = body,
                                "received Matrix message"
                            );
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
                                    if let Err(err) = self
                                        .send_message(
                                            room_id,
                                            &response,
                                            event.event_id.as_deref(),
                                        )
                                        .await
                                    {
                                        error!(?err, "failed to send Matrix response");
                                    }
                                }
                                Ok(None) => {}
                                Err(err) => {
                                    error!(?err, "command failed");
                                    let _ = self
                                        .send_message(
                                            room_id,
                                            &format!("Error: {err:#}"),
                                            event.event_id.as_deref(),
                                        )
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
        rpc_chain: SubxtKusama,
    ) where
        C: ChainData + 'static,
    {
        let mut last_period: Option<CandidatePeriodKind> = None;
        loop {
            match period_snapshot(society.as_ref()).await {
                Ok((period_kind, message)) => {
                    if let Some(last_period) = last_period
                        && last_period != period_kind
                        && let Err(err) = self.send_message(&room_id, &message, None).await
                    {
                        error!(?err, "failed to announce period change");
                    }
                    last_period = Some(period_kind);
                }
                Err(err) => {
                    error!(?err, "failed to poll candidate period; reconnecting RPC");
                    rpc_chain.reconnect().await;
                }
            }
            sleep(Duration::from_secs(60)).await;
        }
    }

    pub async fn announce_society_events(
        &self,
        room_id: String,
        society: Arc<Society<SubxtKusama>>,
        chain: SubxtKusama,
        seen_events: Arc<SeenSocietyEvents>,
    ) {
        chain
            .watch_society_events(|event| {
                let room_id = room_id.clone();
                let society = society.clone();
                let seen_events = seen_events.clone();
                let matrix = self.clone();
                async move {
                    if !seen_events.mark_seen(event.id()) {
                        return Ok(());
                    }
                    match event {
                        SocietyEvent::Bid {
                            block_number,
                            address,
                            bid_plancks,
                            ..
                        } => {
                            let address_or_handle =
                                society.format_account_display(&address).await?;
                            matrix
                                .send_message(
                                    &room_id,
                                    &new_bid_message(
                                        block_number,
                                        &Bid {
                                            address_or_handle,
                                            bid_plancks,
                                        },
                                    ),
                                    None,
                                )
                                .await?;
                        }
                        SocietyEvent::Unbid {
                            block_number,
                            address,
                            ..
                        } => {
                            let address_or_handle =
                                society.format_account_display(&address).await?;
                            matrix
                                .send_message(
                                    &room_id,
                                    &unbid_message(block_number, &address_or_handle),
                                    None,
                                )
                                .await?;
                        }
                    }
                    Ok(())
                }
            })
            .await;
    }
}

async fn period_snapshot<C>(society: &Society<C>) -> Result<(CandidatePeriodKind, String)>
where
    C: ChainData,
{
    let period = society.get_candidate_period().await?;
    let defender = society.get_defending().await?;
    let candidates = society.get_candidates().await?;
    let head = society.get_head_display().await?;
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

fn parse_room_target(value: &str) -> Result<RoomTarget> {
    if value.starts_with('!') {
        return Ok(RoomTarget::RoomId(value.to_owned()));
    }
    if value.starts_with('#') {
        return Ok(RoomTarget::RoomAlias(value.to_owned()));
    }
    anyhow::bail!("MATRIX_ROOM must start with '!' for a room ID or '#' for a room alias");
}

fn message_payload(body: &str, in_reply_to: Option<&str>) -> serde_json::Value {
    let mut payload = json!({
        "msgtype": "m.text",
        "body": strip_markdown(body),
        "format": "org.matrix.custom.html",
        "formatted_body": markdown_to_html(body),
    });
    if let Some(event_id) = in_reply_to {
        payload["m.relates_to"] = json!({
            "m.in_reply_to": {
                "event_id": event_id,
            }
        });
    }
    payload
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            ch => out.push(ch),
        }
    }
    out
}

fn read_until_delimiter(input: &str, start: usize, delimiter: &str) -> Option<(String, usize)> {
    let end = input[start..].find(delimiter)? + start;
    Some((input[start..end].to_owned(), end + delimiter.len()))
}

fn markdown_to_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index..].starts_with("**")
            && let Some((content, next)) = read_until_delimiter(input, index + 2, "**")
        {
            out.push_str("<strong>");
            out.push_str(&escape_html(&content));
            out.push_str("</strong>");
            index = next;
            continue;
        }
        if input.as_bytes()[index] == b'`'
            && let Some((content, next)) = read_until_delimiter(input, index + 1, "`")
        {
            out.push_str("<code>");
            out.push_str(&escape_html(&content));
            out.push_str("</code>");
            index = next;
            continue;
        }
        if input.as_bytes()[index] == b'\n' {
            out.push_str("<br/>");
            index += 1;
            continue;
        }
        let ch = input[index..].chars().next().expect("valid utf-8");
        out.push_str(&escape_html(ch.to_string().as_str()));
        index += ch.len_utf8();
    }
    out
}

fn strip_markdown(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index..].starts_with("**")
            && let Some((content, next)) = read_until_delimiter(input, index + 2, "**")
        {
            out.push_str(&content);
            index = next;
            continue;
        }
        if input.as_bytes()[index] == b'`'
            && let Some((content, next)) = read_until_delimiter(input, index + 1, "`")
        {
            out.push_str(&content);
            index = next;
            continue;
        }
        let ch = input[index..].chars().next().expect("valid utf-8");
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

#[derive(Debug, Clone)]
pub struct RoomTimelineEvent {
    pub event_id: String,
    pub sender: String,
    pub event_type: String,
    pub origin_server_ts: u64,
    pub content: serde_json::Value,
}

pub fn mxc_http_url(homeserver: &Url, mxc_url: &str) -> Result<Url> {
    let rest = mxc_url
        .strip_prefix("mxc://")
        .with_context(|| format!("invalid mxc url {mxc_url}"))?;
    let (server, media_id) = rest
        .split_once('/')
        .with_context(|| format!("invalid mxc url {mxc_url}"))?;
    Ok(homeserver.join(&format!(
        "/_matrix/media/v3/download/{}/{}",
        url_escape(server),
        url_escape(media_id)
    ))?)
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    chunk: Vec<RoomTimelineEventRaw>,
    #[serde(default, rename = "start")]
    _start: String,
    end: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RoomTimelineEventRaw {
    event_id: String,
    sender: String,
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    origin_server_ts: u64,
    #[serde(default)]
    content: serde_json::Value,
}

impl From<RoomTimelineEventRaw> for RoomTimelineEvent {
    fn from(value: RoomTimelineEventRaw) -> Self {
        Self {
            event_id: value.event_id,
            sender: value.sender,
            event_type: value.event_type,
            origin_server_ts: value.origin_server_ts,
            content: value.content,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SyncResponse {
    next_batch: String,
    #[serde(default)]
    rooms: Rooms,
}

#[derive(Debug, Deserialize)]
struct RoomAliasResponse {
    room_id: String,
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
    #[serde(default)]
    event_id: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Society;
    use crate::chain::tests::FakeChain;
    use crate::models::{Candidate, Tally};
    use crate::store::OverrideStore;
    use tempfile::NamedTempFile;

    fn test_society(chain: FakeChain) -> Society<FakeChain> {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        Society::new(chain, OverrideStore::open(path).unwrap())
    }

    #[test]
    fn escapes_matrix_path_segments() {
        assert_eq!(url_escape("abcXYZ-_.~"), "abcXYZ-_.~");
        assert_eq!(url_escape("!room:e2e.local"), "%21room%3Ae2e.local");
        assert_eq!(url_escape("space here"), "space%20here");
    }

    #[test]
    fn parses_room_ids_and_aliases() {
        assert_eq!(
            parse_room_target("!room:e2e.local").unwrap(),
            RoomTarget::RoomId("!room:e2e.local".to_owned())
        );
        assert_eq!(
            parse_room_target("#room:e2e.local").unwrap(),
            RoomTarget::RoomAlias("#room:e2e.local".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_room_target() {
        assert!(
            parse_room_target("room:e2e.local")
                .unwrap_err()
                .to_string()
                .contains("MATRIX_ROOM")
        );
    }

    #[tokio::test]
    async fn returns_room_id_targets_without_lookup() {
        let client = MatrixClient::new(
            "https://matrix.example.org",
            "token".to_owned(),
            "@bot:e2e.local".to_owned(),
        )
        .unwrap();
        let room_id = client.resolve_room_id("!room:e2e.local").await.unwrap();
        assert_eq!(room_id, "!room:e2e.local");
    }

    #[test]
    fn builds_room_alias_lookup_url() {
        let client = MatrixClient::new(
            "https://matrix.example.org",
            "token".to_owned(),
            "@bot:e2e.local".to_owned(),
        )
        .unwrap();
        let url = client.room_alias_url("#society:e2e.local").unwrap();
        assert_eq!(
            url.as_str(),
            "https://matrix.example.org/_matrix/client/v3/directory/room/%23society%3Ae2e.local"
        );
    }

    #[test]
    fn deserializes_sync_response_with_defaults() {
        let response: SyncResponse = serde_json::from_value(serde_json::json!({
            "next_batch": "s1",
            "rooms": {
                "join": {
                    "!room:e2e.local": {
                        "timeline": {
                            "events": [
                                {
                                    "sender": "@user:e2e.local",
                                    "origin_server_ts": 123,
                                    "content": {"body": "!ping"}
                                },
                                {
                                    "sender": "@empty:e2e.local",
                                    "content": {}
                                }
                            ]
                        }
                    }
                }
            }
        }))
        .unwrap();

        let room = response.rooms.join.get("!room:e2e.local").unwrap();
        assert_eq!(response.next_batch, "s1");
        assert_eq!(room.timeline.events.len(), 2);
        assert_eq!(
            room.timeline.events[0].content.body.as_deref(),
            Some("!ping")
        );
        assert_eq!(room.timeline.events[1].origin_server_ts, None);
    }

    #[tokio::test]
    async fn builds_period_snapshot_message() {
        let mut chain = FakeChain {
            block_number: 1,
            head: Some("@head:matrix.org".to_owned()),
            defender: Some("@defender:matrix.org".to_owned()),
            defender_skeptic: Some("@defender-skeptic:matrix.org".to_owned()),
            candidate_skeptic: Some("@candidate-skeptic:matrix.org".to_owned()),
            ..Default::default()
        };
        chain.candidates.push(Candidate {
            address_or_handle: "@candidate:matrix.org".to_owned(),
            bid_plancks: 1_000_000_000_000,
            tally: Tally {
                approvals: 1,
                rejections: 0,
            },
        });
        let society = test_society(chain);

        let (kind, message) = period_snapshot(&society).await.unwrap();
        assert_eq!(kind, CandidatePeriodKind::Voting);
        assert!(message.contains("**New voting period started**"));
        assert!(message.contains("@candidate:matrix.org"));
    }

    #[test]
    fn builds_matrix_urls_and_payloads() {
        let client = MatrixClient::new(
            "https://matrix.example",
            "token".to_owned(),
            "@bot:e2e.local".to_owned(),
        )
        .unwrap();

        assert_eq!(
            client
                .send_message_url("!room:e2e.local", "txn-1")
                .unwrap()
                .as_str(),
            "https://matrix.example/_matrix/client/v3/rooms/%21room%3Ae2e.local/send/m.room.message/txn-1"
        );
        assert_eq!(
            client.sync_url(None).unwrap().as_str(),
            "https://matrix.example/_matrix/client/v3/sync?timeout=30000"
        );
        assert_eq!(
            client.sync_url(Some("s0")).unwrap().as_str(),
            "https://matrix.example/_matrix/client/v3/sync?timeout=30000&since=s0"
        );
        assert_eq!(
            message_payload("hello", None),
            serde_json::json!({
                "msgtype": "m.text",
                "body": "hello",
                "format": "org.matrix.custom.html",
                "formatted_body": "hello",
            })
        );
        let payload = message_payload("**Head:** `alice`\n· item", None);
        assert_eq!(payload["body"], "Head: alice\n· item");
        assert_eq!(
            payload["formatted_body"],
            "<strong>Head:</strong> <code>alice</code><br/>· item"
        );
        assert_eq!(
            message_payload("Pong!", Some("$event123")),
            serde_json::json!({
                "msgtype": "m.text",
                "body": "Pong!",
                "format": "org.matrix.custom.html",
                "formatted_body": "Pong!",
                "m.relates_to": {
                    "m.in_reply_to": {
                        "event_id": "$event123",
                    }
                },
            })
        );
    }

    #[test]
    fn builds_mxc_download_urls() {
        let homeserver = Url::parse("https://matrix.example.org").unwrap();
        let url = mxc_http_url(&homeserver, "mxc://parity.io/abc123").unwrap();
        assert_eq!(
            url.as_str(),
            "https://matrix.example.org/_matrix/media/v3/download/parity.io/abc123"
        );
    }

    #[test]
    fn escapes_html_in_matrix_messages() {
        let payload = message_payload("**Alert** `<script>`", None);
        assert_eq!(
            payload["formatted_body"],
            "<strong>Alert</strong> <code>&lt;script&gt;</code>"
        );
    }
}
