use crate::chain::{ChainData, Society, SubxtKusama};
use crate::matrix::MatrixClient;
use crate::messages::{
    auto_unbid_message, candidate_suspended_message, challenged_message, claim_started_message,
    defender_vote_message, elevated_message, inducted_message, member_suspended_message,
    new_bid_message, period_message, suspended_member_judgement_message, unbid_message,
    unvouch_message, vote_message, vouch_message, x_bid_message, x_challenged_message,
    x_claim_start_message, x_inducted_message, x_round_start_message, x_unbid_message,
    x_vouch_message,
};
use crate::models::{Bid, CandidatePeriodKind, SocietyEvent};
use crate::store::OverrideStore;
use crate::x::XWebhook;
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};

const PERIOD_POLL_INTERVAL: Duration = Duration::from_secs(60);

pub async fn run_period_loop(
    matrix: MatrixClient,
    room_id: String,
    store: Arc<Mutex<OverrideStore>>,
    society: Arc<Society<SubxtKusama>>,
    rpc_chain: SubxtKusama,
    x: Option<XWebhook>,
) -> ! {
    let mut last_period: Option<CandidatePeriodKind> = None;
    loop {
        match period_snapshot(society.as_ref()).await {
            Ok((period_kind, relay_block, message)) => {
                if let Some(previous) = last_period
                    && previous != period_kind
                {
                    match period_kind {
                        CandidatePeriodKind::Voting => {
                            announce_round_start(
                                &matrix,
                                &room_id,
                                &store,
                                x.as_ref(),
                                relay_block,
                                &message,
                                society.as_ref(),
                            )
                            .await;
                        }
                        CandidatePeriodKind::Claim => {
                            announce_claim_start(
                                &matrix,
                                &room_id,
                                &store,
                                x.as_ref(),
                                relay_block,
                                society.as_ref(),
                            )
                            .await;
                        }
                    }
                }
                last_period = Some(period_kind);
            }
            Err(err) => {
                error!(?err, "failed to poll candidate period; reconnecting RPC");
                rpc_chain.reconnect().await;
            }
        }
        sleep(PERIOD_POLL_INTERVAL).await;
    }
}

pub async fn run_event_loop(
    matrix: MatrixClient,
    room_id: String,
    store: Arc<Mutex<OverrideStore>>,
    society: Arc<Society<SubxtKusama>>,
    chain: SubxtKusama,
    x: Option<XWebhook>,
) -> ! {
    let identities = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    chain
        .watch_society_events(|event| {
            let room_id = room_id.clone();
            let society = society.clone();
            let store = store.clone();
            let matrix = matrix.clone();
            let x = x.clone();
            let identities = identities.clone();
            async move {
                if let Err(err) = handle_event(
                    &matrix,
                    &room_id,
                    &store,
                    society.as_ref(),
                    x.as_ref(),
                    &identities,
                    event,
                )
                .await
                {
                    error!(?err, "failed to announce society event");
                }
                Ok(())
            }
        })
        .await
}

async fn period_snapshot<C>(society: &Society<C>) -> Result<(CandidatePeriodKind, u64, String)>
where
    C: ChainData,
{
    let period = society.get_candidate_period().await?;
    let relay_block = society.get_relay_block_number().await?;
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
    Ok((period.kind, relay_block, message))
}

async fn announce_round_start(
    matrix: &MatrixClient,
    room_id: &str,
    store: &Mutex<OverrideStore>,
    x: Option<&XWebhook>,
    relay_block: u64,
    message: &str,
    society: &Society<SubxtKusama>,
) {
    let candidate_count = match society.get_candidates().await {
        Ok(candidates) => candidates.len(),
        Err(err) => {
            error!(?err, "failed to count candidates for X round-start post");
            0
        }
    };
    let duration = match society.get_candidate_period().await {
        Ok(period) => period.voting_time_left(),
        Err(err) => {
            error!(?err, "failed to read voting time for X round-start post");
            Duration::ZERO
        }
    };

    match matrix.send_message(room_id, message, None).await {
        Ok(event_id) => {
            info!(event_id, "stored round root for new voting period");
            if let Err(err) =
                lock_store(store).and_then(|store| store.set_round_root(&event_id, relay_block))
            {
                error!(?err, "failed to persist round root");
            }
        }
        Err(err) => {
            error!(
                ?err,
                "failed to send round start message; no round root until next round"
            );
        }
    }
    dispatch_x(
        x,
        Some(&x_round_start_message(
            relay_block,
            candidate_count,
            duration,
        )),
    );
}

async fn announce_claim_start(
    matrix: &MatrixClient,
    room_id: &str,
    store: &Mutex<OverrideStore>,
    x: Option<&XWebhook>,
    relay_block: u64,
    society: &Society<SubxtKusama>,
) {
    let candidates = match society.get_candidates().await {
        Ok(candidates) => candidates,
        Err(err) => {
            error!(?err, "failed to read candidates for claim-start message");
            return;
        }
    };
    let period = match society.get_candidate_period().await {
        Ok(period) => period,
        Err(err) => {
            error!(?err, "failed to read period for claim-start message");
            return;
        }
    };
    let message = claim_started_message(&period, &candidates);
    let root = lock_store(store)
        .ok()
        .and_then(|store| store.round_root().ok().flatten())
        .map(|root| root.root_event_id);
    dispatch(
        matrix,
        room_id,
        x,
        root.as_deref(),
        message,
        Some(&x_claim_start_message(relay_block)),
    )
    .await;
}

async fn handle_event(
    matrix: &MatrixClient,
    room_id: &str,
    store: &Mutex<OverrideStore>,
    society: &Society<SubxtKusama>,
    x: Option<&XWebhook>,
    identities: &Mutex<HashMap<String, String>>,
    event: SocietyEvent,
) -> Result<()> {
    let (block_hash, event_index, kind) = event.id().event_key();
    if !lock_store(store)?.mark_society_event_seen(&block_hash, event_index, kind)? {
        return Ok(());
    }

    let root = lock_store(store)?
        .round_root()?
        .map(|root| root.root_event_id);

    match event {
        SocietyEvent::Bid {
            block_number,
            address,
            bid_plancks,
            ..
        } => {
            let display = society.format_account_display(&address).await?;
            let text = new_bid_message(
                block_number,
                &Bid {
                    address_or_handle: display.clone(),
                    bid_plancks,
                },
            );
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                text,
                Some(&x_bid_message(block_number, &display, bid_plancks)),
            )
            .await;
        }
        SocietyEvent::Unbid {
            block_number,
            address,
            ..
        } => {
            let display = society.format_account_display(&address).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                unbid_message(block_number, &display),
                Some(&x_unbid_message(block_number, &display)),
            )
            .await;
        }
        SocietyEvent::Vouch {
            block_number,
            candidate,
            offer_plancks,
            voucher,
            ..
        } => {
            let candidate = format_display(society, identities, &candidate).await?;
            let voucher = format_display(society, identities, &voucher).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                vouch_message(block_number, &candidate, &voucher, offer_plancks),
                Some(&x_vouch_message(
                    block_number,
                    &voucher,
                    &candidate,
                    offer_plancks,
                )),
            )
            .await;
        }
        SocietyEvent::Unvouch {
            block_number,
            candidate,
            ..
        } => {
            let candidate = format_display(society, identities, &candidate).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                unvouch_message(block_number, &candidate),
                None,
            )
            .await;
        }
        SocietyEvent::AutoUnbid {
            block_number,
            candidate,
            ..
        } => {
            let candidate = format_display(society, identities, &candidate).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                auto_unbid_message(block_number, &candidate),
                None,
            )
            .await;
        }
        SocietyEvent::Inducted {
            block_number,
            primary,
            candidates,
            ..
        } => {
            let primary = format_display(society, identities, &primary).await?;
            let mut candidates_display = Vec::with_capacity(candidates.len());
            for candidate in &candidates {
                candidates_display.push(format_display(society, identities, candidate).await?);
            }
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                inducted_message(block_number, &primary, &candidates_display),
                Some(&x_inducted_message(
                    block_number,
                    candidates_display.len(),
                    &primary,
                )),
            )
            .await;
        }
        SocietyEvent::Challenged {
            block_number,
            member,
            ..
        } => {
            let member = format_display(society, identities, &member).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                challenged_message(block_number, &member),
                Some(&x_challenged_message(block_number, &member)),
            )
            .await;
        }
        SocietyEvent::CandidateSuspended {
            block_number,
            candidate,
            ..
        } => {
            let candidate = format_display(society, identities, &candidate).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                candidate_suspended_message(block_number, &candidate),
                None,
            )
            .await;
        }
        SocietyEvent::MemberSuspended {
            block_number,
            member,
            ..
        } => {
            let member = format_display(society, identities, &member).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                member_suspended_message(block_number, &member),
                None,
            )
            .await;
        }
        SocietyEvent::SuspendedMemberJudgement {
            block_number,
            who,
            judged,
            ..
        } => {
            let who = format_display(society, identities, &who).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                suspended_member_judgement_message(block_number, &who, judged),
                None,
            )
            .await;
        }
        SocietyEvent::Elevated {
            block_number,
            member,
            rank,
            ..
        } => {
            let member = format_display(society, identities, &member).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                elevated_message(block_number, &member, rank),
                None,
            )
            .await;
        }
        SocietyEvent::Vote {
            block_number,
            candidate,
            voter,
            approve,
            ..
        } => {
            let candidate = format_display(society, identities, &candidate).await?;
            let voter = format_display(society, identities, &voter).await?;
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                vote_message(block_number, &voter, approve, &candidate),
                None,
            )
            .await;
        }
        SocietyEvent::DefenderVote {
            block_number,
            voter,
            approve,
            ..
        } => {
            let voter = format_display(society, identities, &voter).await?;
            let defender = match society.get_defending().await?.address_or_handle {
                Some(defender) => defender,
                None => "unknown".to_owned(),
            };
            dispatch(
                matrix,
                room_id,
                x,
                root.as_deref(),
                defender_vote_message(block_number, &voter, approve, &defender),
                None,
            )
            .await;
        }
    }
    Ok(())
}

/// Sends the Matrix message (thread when a round root exists, main channel otherwise)
/// and posts the optional X text without ever blocking or failing the Matrix path.
async fn dispatch(
    matrix: &MatrixClient,
    room_id: &str,
    x: Option<&XWebhook>,
    root_event_id: Option<&str>,
    text: String,
    x_text: Option<&str>,
) {
    match root_event_id {
        Some(root) => {
            if let Err(err) = matrix.send_thread_message(room_id, &text, root).await {
                error!(?err, "failed to send thread message");
            }
        }
        None => {
            warn!("no round root stored; falling back to main channel");
            if let Err(err) = matrix.send_message(room_id, &text, None).await {
                error!(?err, "failed to send message to main channel");
            }
        }
    }
    dispatch_x(x, x_text);
}

fn dispatch_x(x: Option<&XWebhook>, text: Option<&str>) {
    if let (Some(x), Some(text)) = (x, text) {
        let x = x.clone();
        let text = text.to_owned();
        tokio::spawn(async move {
            if let Err(err) = x.post(&text).await {
                error!(?err, "X webhook post failed");
            }
        });
    }
}

async fn format_display(
    society: &Society<SubxtKusama>,
    identities: &Mutex<HashMap<String, String>>,
    address: &str,
) -> Result<String> {
    if let Some(name) = lock_store(identities)?.get(address).cloned() {
        return Ok(name);
    }
    let name = society.format_display_name(address).await?;
    lock_store(identities)?.insert(address.to_owned(), name.clone());
    Ok(name)
}

fn lock_store<T>(guard: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    guard
        .lock()
        .map_err(|_| anyhow!("override store lock poisoned"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::tests::FakeChain;
    use crate::models::{Candidate, CandidatePeriodKind, Tally};
    use tempfile::NamedTempFile;

    fn test_store() -> Arc<Mutex<OverrideStore>> {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        Arc::new(Mutex::new(OverrideStore::open(path).unwrap()))
    }

    #[tokio::test]
    async fn builds_period_snapshot_message() {
        let mut chain = FakeChain {
            block_number: 1,
            relay_block_number: 1000,
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
        let society = Society::new(chain, test_store());

        let (kind, relay_block, message) = period_snapshot(&society).await.unwrap();
        assert_eq!(kind, CandidatePeriodKind::Voting);
        assert_eq!(relay_block, 1000);
        assert!(message.contains("**New voting period started**"));
        assert!(message.contains("@candidate:matrix.org"));
    }
}
