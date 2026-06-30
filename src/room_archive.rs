use crate::matrix::{MatrixClient, RoomTimelineEvent, mxc_http_url};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub output_dir: PathBuf,
    pub download_media: bool,
    pub include_all_events: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ExportStats {
    pub events: u64,
    pub media_links: u64,
    pub media_downloaded: u64,
    pub media_failed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedEvent {
    pub event_id: String,
    pub sender: String,
    pub origin_server_ts: u64,
    pub event_type: String,
    pub msgtype: Option<String>,
    pub body: Option<String>,
    pub mxc_url: Option<String>,
    pub http_url: Option<String>,
    pub media_path: Option<String>,
    pub content: serde_json::Value,
}

pub async fn export_room_history(
    client: &MatrixClient,
    room_id: &str,
    room_label: &str,
    options: ExportOptions,
) -> Result<ExportStats> {
    std::fs::create_dir_all(&options.output_dir)
        .with_context(|| format!("creating export directory {}", options.output_dir.display()))?;
    let media_dir = options.output_dir.join("media");
    if options.download_media {
        std::fs::create_dir_all(&media_dir)
            .with_context(|| format!("creating media directory {}", media_dir.display()))?;
    }

    let event_types = if options.include_all_events {
        None
    } else {
        Some(vec!["m.room.message".to_owned()])
    };
    let events = client.fetch_all_room_events(room_id, event_types).await?;
    info!(events = events.len(), "exporting room events");

    let mut stats = ExportStats::default();
    let mut lines = Vec::with_capacity(events.len());

    for event in events {
        stats.events += 1;
        let exported = export_event(
            client,
            &event,
            &media_dir,
            options.download_media,
            &mut stats,
        )
        .await?;
        lines.push(serde_json::to_string(&exported)?);
    }

    let events_path = options.output_dir.join("events.jsonl");
    tokio::fs::write(&events_path, lines.join("\n") + "\n")
        .await
        .with_context(|| format!("writing {}", events_path.display()))?;

    let metadata = serde_json::json!({
        "room_id": room_id,
        "room_label": room_label,
        "events": stats.events,
        "media_links": stats.media_links,
        "media_downloaded": stats.media_downloaded,
        "media_failed": stats.media_failed,
        "download_media": options.download_media,
        "include_all_events": options.include_all_events,
    });
    let metadata_path = options.output_dir.join("metadata.json");
    tokio::fs::write(
        &metadata_path,
        serde_json::to_string_pretty(&metadata)? + "\n",
    )
    .await
    .with_context(|| format!("writing {}", metadata_path.display()))?;

    info!(
        events = stats.events,
        media_links = stats.media_links,
        media_downloaded = stats.media_downloaded,
        output = %options.output_dir.display(),
        "room export complete"
    );
    Ok(stats)
}

pub fn load_history_messages(
    export_dir: &Path,
) -> Result<Vec<crate::overrides_import::HistoryMessage>> {
    use crate::overrides_import::HistoryMessage;
    let events_path = export_dir.join("events.jsonl");
    let raw = std::fs::read_to_string(&events_path)
        .with_context(|| format!("reading {}", events_path.display()))?;
    let mut messages = Vec::new();
    for (line_number, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event: ExportedEvent = serde_json::from_str(line).with_context(|| {
            format!(
                "parsing export event at line {} in {}",
                line_number + 1,
                events_path.display()
            )
        })?;
        let Some(body) = event.body else {
            continue;
        };
        messages.push(HistoryMessage {
            sender: event.sender,
            body,
            origin_server_ts: event.origin_server_ts,
        });
    }
    Ok(messages)
}

pub fn load_export_events(export_dir: &Path) -> Result<Vec<ExportedEvent>> {
    let events_path = export_dir.join("events.jsonl");
    let raw = std::fs::read_to_string(&events_path)
        .with_context(|| format!("reading {}", events_path.display()))?;
    let mut events = Vec::new();
    for (line_number, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        events.push(serde_json::from_str(line).with_context(|| {
            format!(
                "parsing export event at line {} in {}",
                line_number + 1,
                events_path.display()
            )
        })?);
    }
    Ok(events)
}

pub async fn retry_failed_media(client: &MatrixClient, output_dir: &Path) -> Result<ExportStats> {
    let media_dir = output_dir.join("media");
    std::fs::create_dir_all(&media_dir)
        .with_context(|| format!("creating media directory {}", media_dir.display()))?;

    let mut events = load_export_events(output_dir)?;
    let mut stats = ExportStats::default();
    let mut retried = 0_u64;

    for event in &mut events {
        if event.mxc_url.is_none() {
            continue;
        }
        stats.media_links += 1;
        if media_is_present(output_dir, event) {
            stats.media_downloaded += 1;
            continue;
        }

        retried += 1;
        match download_exported_media(client, output_dir, &media_dir, event).await {
            Ok(()) => stats.media_downloaded += 1,
            Err(err) => {
                stats.media_failed += 1;
                warn!(event_id = %event.event_id, ?err, "failed to download media");
            }
        }
    }

    stats.events = events.len() as u64;
    save_export_events(output_dir, &events)?;
    update_metadata(output_dir, &stats, true).await?;

    info!(
        events = stats.events,
        retried,
        media_links = stats.media_links,
        media_downloaded = stats.media_downloaded,
        media_failed = stats.media_failed,
        output = %output_dir.display(),
        "retried failed media"
    );
    Ok(stats)
}

fn save_export_events(export_dir: &Path, events: &[ExportedEvent]) -> Result<()> {
    let events_path = export_dir.join("events.jsonl");
    let mut lines = Vec::with_capacity(events.len());
    for event in events {
        lines.push(serde_json::to_string(event)?);
    }
    std::fs::write(&events_path, lines.join("\n") + "\n")
        .with_context(|| format!("writing {}", events_path.display()))?;
    Ok(())
}

async fn update_metadata(output_dir: &Path, stats: &ExportStats, retry_failed: bool) -> Result<()> {
    let metadata_path = output_dir.join("metadata.json");
    let mut metadata = if metadata_path.exists() {
        let raw = std::fs::read_to_string(&metadata_path)
            .with_context(|| format!("reading {}", metadata_path.display()))?;
        serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert("events".into(), stats.events.into());
        obj.insert("media_links".into(), stats.media_links.into());
        obj.insert("media_downloaded".into(), stats.media_downloaded.into());
        obj.insert("media_failed".into(), stats.media_failed.into());
        obj.insert("retry_failed".into(), retry_failed.into());
    }
    tokio::fs::write(
        &metadata_path,
        serde_json::to_string_pretty(&metadata)? + "\n",
    )
    .await
    .with_context(|| format!("writing {}", metadata_path.display()))?;
    Ok(())
}

fn media_is_present(output_dir: &Path, event: &ExportedEvent) -> bool {
    let Some(media_path) = event.media_path.as_deref() else {
        return false;
    };
    output_dir.join(media_path).is_file()
}

async fn download_exported_media(
    client: &MatrixClient,
    output_dir: &Path,
    media_dir: &Path,
    event: &mut ExportedEvent,
) -> Result<()> {
    let mxc_url = event.mxc_url.as_deref().context("event has no mxc url")?;
    let filename = media_filename(&event.event_id, &event.content, event.msgtype.as_deref());
    let destination = media_dir.join(&filename);
    client
        .download_mxc(mxc_url, &destination)
        .await
        .with_context(|| format!("downloading {mxc_url}"))?;
    event.media_path = Some(format!("media/{filename}"));
    event.http_url = mxc_http_url(client.homeserver(), mxc_url)
        .ok()
        .map(|url| url.to_string());
    info!(
        event_id = %event.event_id,
        path = %destination.display(),
        "downloaded media"
    );
    Ok(())
}

async fn export_event(
    client: &MatrixClient,
    event: &RoomTimelineEvent,
    media_dir: &Path,
    download_media: bool,
    stats: &mut ExportStats,
) -> Result<ExportedEvent> {
    let msgtype = event
        .content
        .get("msgtype")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let body = event
        .content
        .get("body")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let mxc_url = media_mxc_url(&event.content);
    let http_url = mxc_url
        .as_deref()
        .map(|mxc| mxc_http_url(client.homeserver(), mxc))
        .transpose()?
        .map(|url| url.to_string());

    let mut media_path = None;
    if let Some(mxc_url) = &mxc_url {
        stats.media_links += 1;
        if download_media {
            let filename = media_filename(&event.event_id, &event.content, msgtype.as_deref());
            let relative_path = format!("media/{filename}");
            let destination = media_dir.join(&filename);
            let already_present = destination.is_file();
            if already_present {
                stats.media_downloaded += 1;
                media_path = Some(relative_path);
            } else {
                match client
                    .download_mxc(mxc_url, &destination)
                    .await
                    .with_context(|| format!("downloading {mxc_url}"))
                {
                    Ok(()) => {
                        stats.media_downloaded += 1;
                        media_path = Some(relative_path);
                        info!(
                            event_id = %event.event_id,
                            path = %destination.display(),
                            "downloaded media"
                        );
                    }
                    Err(err) => {
                        stats.media_failed += 1;
                        warn!(event_id = %event.event_id, ?err, "failed to download media");
                    }
                }
            }
        }
    }

    Ok(ExportedEvent {
        event_id: event.event_id.clone(),
        sender: event.sender.clone(),
        origin_server_ts: event.origin_server_ts,
        event_type: event.event_type.clone(),
        msgtype,
        body,
        mxc_url,
        http_url,
        media_path,
        content: event.content.clone(),
    })
}

fn media_mxc_url(content: &serde_json::Value) -> Option<String> {
    content
        .get("url")
        .and_then(serde_json::Value::as_str)
        .filter(|url| url.starts_with("mxc://"))
        .map(str::to_owned)
}

fn media_filename(event_id: &str, content: &serde_json::Value, msgtype: Option<&str>) -> String {
    let preferred = content
        .get("filename")
        .and_then(serde_json::Value::as_str)
        .or_else(|| content.get("body").and_then(serde_json::Value::as_str))
        .unwrap_or_else(|| msgtype.unwrap_or("file"));
    let base = sanitize_component(event_id);
    let name = sanitize_component(preferred);
    if name.contains('.') {
        format!("{base}_{name}")
    } else {
        format!("{base}_{name}.{}", default_extension(msgtype))
    }
}

fn default_extension(msgtype: Option<&str>) -> &str {
    match msgtype {
        Some("m.image") => "jpg",
        Some("m.video") => "mp4",
        Some("m.audio") => "ogg",
        _ => "bin",
    }
}

fn sanitize_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "file".to_owned()
    } else {
        trimmed.chars().take(80).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_history_messages_from_export_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("events.jsonl"),
            r#"{"event_id":"$1:parity.io","sender":"@alice:parity.io","origin_server_ts":1,"event_type":"m.room.message","msgtype":"m.text","body":"!set_address FUfBKr2pDxKrxmExGp4hjU6St4BDgffzKcyAqv6pruGnez1","mxc_url":null,"http_url":null,"media_path":null,"content":{}}
"#,
        )
        .unwrap();
        let messages = load_history_messages(dir.path()).unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].body.starts_with("!set_address"));
    }

    #[test]
    fn extracts_mxc_urls_from_media_messages() {
        let content = serde_json::json!({
            "msgtype": "m.image",
            "body": "photo.jpg",
            "url": "mxc://parity.io/abc123",
            "info": {"mimetype": "image/jpeg"}
        });
        assert_eq!(
            media_mxc_url(&content).as_deref(),
            Some("mxc://parity.io/abc123")
        );
    }

    #[test]
    fn builds_safe_media_filenames() {
        let content = serde_json::json!({
            "filename": "proof of ink.png",
            "url": "mxc://parity.io/abc123"
        });
        let filename = media_filename("$evt:parity.io", &content, Some("m.image"));
        assert!(filename.starts_with("evt_parity.io_"));
        assert!(filename.ends_with(".png"));
    }

    #[test]
    fn detects_missing_media_files() {
        let dir = tempfile::tempdir().unwrap();
        let event = ExportedEvent {
            event_id: "$1:parity.io".to_owned(),
            sender: "@alice:parity.io".to_owned(),
            origin_server_ts: 1,
            event_type: "m.room.message".to_owned(),
            msgtype: Some("m.image".to_owned()),
            body: Some("photo.png".to_owned()),
            mxc_url: Some("mxc://parity.io/abc".to_owned()),
            http_url: None,
            media_path: Some("media/missing.png".to_owned()),
            content: serde_json::json!({}),
        };
        assert!(!media_is_present(dir.path(), &event));
    }

    #[test]
    fn detects_present_media_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("media")).unwrap();
        std::fs::write(dir.path().join("media/photo.png"), b"data").unwrap();
        let event = ExportedEvent {
            event_id: "$1:parity.io".to_owned(),
            sender: "@alice:parity.io".to_owned(),
            origin_server_ts: 1,
            event_type: "m.room.message".to_owned(),
            msgtype: Some("m.image".to_owned()),
            body: Some("photo.png".to_owned()),
            mxc_url: Some("mxc://parity.io/abc".to_owned()),
            http_url: None,
            media_path: Some("media/photo.png".to_owned()),
            content: serde_json::json!({}),
        };
        assert!(media_is_present(dir.path(), &event));
    }

    #[test]
    fn ignores_messages_without_media_urls() {
        let content = serde_json::json!({
            "msgtype": "m.text",
            "body": "hello"
        });
        assert!(media_mxc_url(&content).is_none());
    }
}
