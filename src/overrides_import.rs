use crate::ss58::{is_valid_address, is_valid_matrix_handle};
use crate::store::OverrideStore;
use anyhow::Result;
use std::collections::VecDeque;

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
    pub me_commands: u64,
    pub me_responses: u64,
    pub me_no_address_responses: u64,
    pub invalid_commands: u64,
    pub final_overrides: u64,
}

const ME_RESPONSE_WINDOW_MS: u64 = 30_000;

pub fn replay_override_history(
    store: &OverrideStore,
    prefix: &str,
    bot_user_id: &str,
    messages: &[HistoryMessage],
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();
    let mut ordered = messages.to_vec();
    ordered.sort_by_key(|message| message.origin_server_ts);
    let mut pending_me = VecDeque::<(String, u64)>::new();

    for message in ordered {
        while pending_me.front().is_some_and(|(_, me_ts)| {
            message.origin_server_ts.saturating_sub(*me_ts) > ME_RESPONSE_WINDOW_MS
        }) {
            pending_me.pop_front();
        }

        if is_me_command(&message.body, prefix) {
            stats.me_commands += 1;
            pending_me.push_back((message.sender.clone(), message.origin_server_ts));
            continue;
        }

        if let Some((matrix_handle, address)) = parse_member_override_from_response(&message.body)
            && let Some(index) = pending_me.iter().rposition(|(requester, me_ts)| {
                *requester == matrix_handle
                    && message.origin_server_ts >= *me_ts
                    && message.origin_server_ts.saturating_sub(*me_ts) <= ME_RESPONSE_WINDOW_MS
            })
        {
            pending_me.remove(index);
            if is_valid_address(&address) && is_valid_matrix_handle(&matrix_handle) {
                stats.me_responses += 1;
                store.unset_by_matrix_handle(&matrix_handle)?;
                store.set_matrix_handle(&address, &matrix_handle)?;
            } else {
                stats.invalid_commands += 1;
            }
            continue;
        }

        if let Some(address) = parse_member_address_from_me_response(&message.body)
            && let Some(index) = pending_me.iter().rposition(|(_, me_ts)| {
                message.origin_server_ts >= *me_ts
                    && message.origin_server_ts.saturating_sub(*me_ts) <= ME_RESPONSE_WINDOW_MS
            })
        {
            let (requester, _) = pending_me.remove(index).expect("index exists");
            if is_valid_address(&address) && is_valid_matrix_handle(&requester) {
                stats.me_responses += 1;
                store.unset_by_matrix_handle(&requester)?;
                store.set_matrix_handle(&address, &requester)?;
            } else {
                stats.invalid_commands += 1;
            }
            continue;
        }

        if is_me_no_address_response(&message.body)
            && let Some(index) = pending_me.iter().rposition(|(_, me_ts)| {
                message.origin_server_ts >= *me_ts
                    && message.origin_server_ts.saturating_sub(*me_ts) <= ME_RESPONSE_WINDOW_MS
            })
        {
            pending_me.remove(index);
            stats.me_no_address_responses += 1;
            continue;
        }

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

fn is_me_command(body: &str, prefix: &str) -> bool {
    body.strip_prefix(prefix)
        .is_some_and(|command| command.trim() == "me")
}

fn is_me_no_address_response(body: &str) -> bool {
    body.contains("not set your address") || body.contains("No address linked yet")
}

fn parse_member_address_from_me_response(body: &str) -> Option<String> {
    if !body.starts_with("**Member**") {
        return None;
    }
    parse_member_address_from_response(body)
}

fn parse_member_address_from_response(body: &str) -> Option<String> {
    for line in body.lines() {
        let line = line.trim();
        let value = if let Some(rest) = line.strip_prefix("* **Address**:") {
            Some(parse_field_value(rest))
        } else if let Some(rest) = line.strip_prefix("**Address**:") {
            Some(parse_field_value(rest))
        } else if let Some(rest) = line.strip_prefix("· Address:") {
            Some(parse_field_value(rest))
        } else if let Some(rest) = line.strip_prefix("Address:") {
            Some(parse_field_value(rest))
        } else {
            None
        };

        if line.contains("Address") {
            let value = value?;
            let (address, _) = split_address_and_handle(&value);
            return Some(address);
        }
    }
    None
}

fn parse_member_override_from_response(body: &str) -> Option<(String, String)> {
    let mut address = None;
    let mut element = None;

    for line in body.lines() {
        let line = line.trim();
        let value = if let Some(rest) = line.strip_prefix("* **Address**:") {
            Some(parse_field_value(rest))
        } else if let Some(rest) = line.strip_prefix("**Address**:") {
            Some(parse_field_value(rest))
        } else if let Some(rest) = line.strip_prefix("· Address:") {
            Some(parse_field_value(rest))
        } else if let Some(rest) = line.strip_prefix("Address:") {
            Some(parse_field_value(rest))
        } else if let Some(rest) = line.strip_prefix("* **Element_handle**:") {
            Some(parse_field_value(rest))
        } else if let Some(rest) = line.strip_prefix("**Element_handle**:") {
            Some(parse_field_value(rest))
        } else if let Some(rest) = line.strip_prefix("· Element:") {
            Some(parse_field_value(rest))
        } else {
            line.strip_prefix("Element:").map(parse_field_value)
        };

        if let Some(value) = value {
            if line.contains("Address") {
                address = Some(value);
            } else {
                element = Some(value);
            }
        }
    }

    let mut address = address?;
    if element.is_none() {
        (address, element) = split_address_and_handle(&address);
    }
    let element = element?;
    if element.eq_ignore_ascii_case("none") || !element.starts_with('@') {
        return None;
    }
    Some((element, address))
}

fn split_address_and_handle(value: &str) -> (String, Option<String>) {
    let Some(open) = value.rfind(" (@") else {
        return (value.to_owned(), None);
    };
    if !value.ends_with(')') {
        return (value.to_owned(), None);
    }
    let address = value[..open].trim().to_owned();
    let handle = value[open + 2..value.len() - 1].trim().to_owned();
    (address, Some(handle))
}

fn parse_field_value(raw: &str) -> String {
    raw.trim().trim_matches('`').trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::OverrideStore;
    use tempfile::NamedTempFile;

    const MEMBER: &str = "FUfBKr2pDxKrxmExGp4hjU6St4BDgffzKcyAqv6pruGnez1";
    const USER: &str = "@alice:parity.io";
    const BOT: &str = "@societybot:matrix.org";

    fn test_store() -> OverrideStore {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        OverrideStore::open(path).unwrap()
    }

    fn me_response_body(user: &str, address: &str) -> String {
        format!(
            "* **Address**: {address}\n\
             * **State**: MemberState.MEMBER\n\
             * **Element_handle**: {user}\n\
             * **Strikes**: 0\n\
             * **Is_founder**: False\n\
             * **Is_defender**: False\n"
        )
    }

    #[test]
    fn replays_set_and_unset_commands_in_order() {
        let store = test_store();
        let stats = replay_override_history(
            &store,
            "!",
            BOT,
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
            BOT,
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

    #[test]
    fn split_address_and_handle_extracts_matrix_id() {
        assert_eq!(
            split_address_and_handle(&format!("{MEMBER} ({USER})")),
            (MEMBER.to_owned(), Some(USER.to_owned()))
        );
        assert_eq!(split_address_and_handle(MEMBER), (MEMBER.to_owned(), None));
    }

    #[test]
    fn parses_old_and_new_me_response_formats() {
        let old = me_response_body(USER, MEMBER);
        assert_eq!(
            parse_member_override_from_response(&old),
            Some((USER.to_owned(), MEMBER.to_owned()))
        );

        let with_handle = format!(
            "**Member**\n· Address: `{MEMBER} ({USER})`\n· Status: member\n· Strikes: 0"
        );
        assert_eq!(
            parse_member_override_from_response(&with_handle),
            Some((USER.to_owned(), MEMBER.to_owned()))
        );

        let without_handle =
            format!("**Member**\n· Address: `{MEMBER}`\n· Status: member\n· Strikes: 0");
        assert_eq!(parse_member_override_from_response(&without_handle), None);
        assert_eq!(
            parse_member_address_from_me_response(&without_handle).as_deref(),
            Some(MEMBER)
        );
        assert_eq!(
            parse_member_override_from_response("* **Address**: abc\n* **Element_handle**: None\n"),
            None
        );
    }

    #[test]
    fn replays_address_only_me_responses() {
        let store = test_store();
        let stats = replay_override_history(
            &store,
            "!",
            BOT,
            &[
                HistoryMessage {
                    sender: USER.to_owned(),
                    body: "!me".to_owned(),
                    origin_server_ts: 1,
                },
                HistoryMessage {
                    sender: BOT.to_owned(),
                    body: format!("**Member**\n· Address: `{MEMBER}`\n· Status: member\n· Strikes: 0"),
                    origin_server_ts: 1_500,
                },
            ],
        )
        .unwrap();

        assert_eq!(stats.me_responses, 1);
        assert_eq!(
            store.address_for_matrix_handle(USER).unwrap().as_deref(),
            Some(MEMBER)
        );
    }

    #[test]
    fn replays_me_responses_after_set_address_commands() {
        let store = test_store();
        let stats = replay_override_history(
            &store,
            "!",
            BOT,
            &[
                HistoryMessage {
                    sender: USER.to_owned(),
                    body: "!me".to_owned(),
                    origin_server_ts: 1,
                },
                HistoryMessage {
                    sender: BOT.to_owned(),
                    body: me_response_body(USER, MEMBER),
                    origin_server_ts: 1_500,
                },
            ],
        )
        .unwrap();

        assert_eq!(stats.me_commands, 1);
        assert_eq!(stats.me_responses, 1);
        assert_eq!(
            store.address_for_matrix_handle(USER).unwrap().as_deref(),
            Some(MEMBER)
        );
    }

    #[test]
    fn me_response_after_set_address_wins_when_later() {
        let store = test_store();
        let other = "G75yJUM2TveDikvysHHW5XhkP35gXqDAsgRLYQTh3gVDir9";
        replay_override_history(
            &store,
            "!",
            BOT,
            &[
                HistoryMessage {
                    sender: USER.to_owned(),
                    body: format!("!set_address {MEMBER}"),
                    origin_server_ts: 1,
                },
                HistoryMessage {
                    sender: USER.to_owned(),
                    body: "!me".to_owned(),
                    origin_server_ts: 2,
                },
                HistoryMessage {
                    sender: BOT.to_owned(),
                    body: me_response_body(USER, other),
                    origin_server_ts: 2_500,
                },
            ],
        )
        .unwrap();

        assert_eq!(
            store.address_for_matrix_handle(USER).unwrap().as_deref(),
            Some(other)
        );
    }

    #[test]
    fn ignores_info_responses_without_pending_me_command() {
        let store = test_store();
        replay_override_history(
            &store,
            "!",
            BOT,
            &[HistoryMessage {
                sender: BOT.to_owned(),
                body: me_response_body(USER, MEMBER),
                origin_server_ts: 1,
            }],
        )
        .unwrap();

        assert_eq!(store.list_overrides().unwrap(), vec![]);
    }
}
