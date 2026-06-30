use crate::ss58::{is_valid_address, is_valid_matrix_handle};
use crate::store::OverrideStore;
use anyhow::Result;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    pub sender: String,
    pub body: String,
    pub origin_server_ts: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportStats {
    pub messages_seen: u64,
    pub set_commands: u64,
    pub unset_commands: u64,
    pub invalid_commands: u64,
    pub final_overrides: u64,
}

pub fn replay_override_history(
    store: &OverrideStore,
    prefix: &str,
    bot_user_id: &str,
    messages: &[HistoryMessage],
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();
    let mut ordered = messages.to_vec();
    ordered.sort_by_key(|message| message.origin_server_ts);

    for message in ordered {
        if message.sender == bot_user_id {
            continue;
        }
        let Some(body) = message.body.strip_prefix(prefix) else {
            continue;
        };
        stats.messages_seen += 1;

        let mut parts = body.split_whitespace();
        let Some(command) = parts.next() else {
            continue;
        };

        match command {
            "set_address" => {
                stats.set_commands += 1;
                let Some(address) = parts.next() else {
                    stats.invalid_commands += 1;
                    continue;
                };
                if !is_valid_address(address) || !is_valid_matrix_handle(&message.sender) {
                    stats.invalid_commands += 1;
                    continue;
                }
                store.unset_by_matrix_handle(&message.sender)?;
                store.set_matrix_handle(address, &message.sender)?;
            }
            "unset_address" => {
                stats.unset_commands += 1;
                if !is_valid_matrix_handle(&message.sender) {
                    stats.invalid_commands += 1;
                    continue;
                }
                if !store.unset_by_matrix_handle(&message.sender)? {
                    stats.invalid_commands += 1;
                }
            }
            _ => {}
        }
    }

    stats.final_overrides = store.list_overrides()?.len() as u64;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::OverrideStore;
    use tempfile::NamedTempFile;

    const MEMBER: &str = "FUfBKr2pDxKrxmExGp4hjU6St4BDgffzKcyAqv6pruGnez1";
    const USER: &str = "@alice:parity.io";

    fn test_store() -> OverrideStore {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        OverrideStore::open(path).unwrap()
    }

    #[test]
    fn replays_set_and_unset_commands_in_order() {
        let store = test_store();
        let stats = replay_override_history(
            &store,
            "!",
            "@bot:parity.io",
            &[
                HistoryMessage {
                    sender: USER.to_owned(),
                    body: format!("!set_address {MEMBER}"),
                    origin_server_ts: 1,
                },
                HistoryMessage {
                    sender: USER.to_owned(),
                    body: format!("!set_address {MEMBER}"),
                    origin_server_ts: 2,
                },
                HistoryMessage {
                    sender: USER.to_owned(),
                    body: "!unset_address".to_owned(),
                    origin_server_ts: 3,
                },
            ],
        )
        .unwrap();

        assert_eq!(stats.set_commands, 2);
        assert_eq!(stats.unset_commands, 1);
        assert_eq!(stats.final_overrides, 0);
        assert_eq!(store.address_for_matrix_handle(USER).unwrap(), None);
    }

    #[test]
    fn keeps_latest_set_for_each_matrix_handle() {
        let store = test_store();
        let other = "G75yJUM2TveDikvysHHW5XhkP35gXqDAsgRLYQTh3gVDir9";
        replay_override_history(
            &store,
            "!",
            "@bot:parity.io",
            &[
                HistoryMessage {
                    sender: USER.to_owned(),
                    body: format!("!set_address {MEMBER}"),
                    origin_server_ts: 1,
                },
                HistoryMessage {
                    sender: USER.to_owned(),
                    body: format!("!set_address {other}"),
                    origin_server_ts: 2,
                },
            ],
        )
        .unwrap();

        assert_eq!(
            store.address_for_matrix_handle(USER).unwrap().as_deref(),
            Some(other)
        );
    }
}
