use crate::models::{
    Bid, Candidate, CandidatePeriod, Defender, MemberInfo, MemberState, SocietyEvent,
    SocietyEventId, SocietyEventKind, Tally,
};
use crate::ss58::{decode_account_id, encode_account_id, is_valid_address, is_valid_matrix_handle};
use crate::store::OverrideStore;
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::future::Future;
use std::sync::{Arc, Mutex};
use subxt::backend::LegacyBackend;
use subxt::dynamic::{Value, storage};
use subxt::error::{Error as SubxtError, StorageError};
use subxt::{OnlineClient, PolkadotConfig};
use subxt_rpcs::RpcClient;
use tracing::info;

const KUSAMA_SS58_PREFIX: u16 = 2;

#[async_trait]
pub trait ChainData: Send + Sync {
    async fn member_addresses(&self) -> Result<Vec<String>>;
    async fn suspended_member_addresses(&self) -> Result<Vec<String>>;
    async fn bids_raw(&self) -> Result<Vec<Bid>>;
    async fn candidates_raw(&self) -> Result<Vec<Candidate>>;
    async fn strikes(&self, address: &str) -> Result<u64>;
    async fn defending_raw(&self) -> Result<Defender>;
    async fn head_address(&self) -> Result<Option<String>>;
    async fn candidate_skeptic(&self) -> Result<Option<String>>;
    async fn founder(&self) -> Result<Option<String>>;
    async fn block_number(&self) -> Result<u64>;
    async fn identity_matrix_handle(&self, address: &str) -> Result<Option<String>>;
}

pub struct Society<C> {
    chain: C,
    store: Arc<Mutex<OverrideStore>>,
}

impl<C> Society<C>
where
    C: ChainData,
{
    pub fn new(chain: C, store: OverrideStore) -> Self {
        Self {
            chain,
            store: Arc::new(Mutex::new(store)),
        }
    }

    pub fn set_matrix_handle(&self, address: &str, matrix_handle: &str) -> Result<bool> {
        if !is_valid_matrix_handle(matrix_handle) || !is_valid_address(address) {
            return Ok(false);
        }
        self.store
            .lock()
            .map_err(|_| anyhow!("override store lock poisoned"))?
            .set_matrix_handle(address, matrix_handle)?;
        Ok(true)
    }

    pub fn unset_matrix_handle(&self, matrix_handle: &str) -> Result<bool> {
        self.store
            .lock()
            .map_err(|_| anyhow!("override store lock poisoned"))?
            .unset_by_matrix_handle(matrix_handle)
    }

    pub async fn get_matrix_handle(&self, address: &str) -> Result<Option<String>> {
        if let Some(handle) = self
            .store
            .lock()
            .map_err(|_| anyhow!("override store lock poisoned"))?
            .matrix_handle_for_address(address)?
        {
            return Ok(Some(handle));
        }
        self.chain.identity_matrix_handle(address).await
    }

    pub fn get_address_for_matrix_handle(&self, matrix_handle: &str) -> Result<Option<String>> {
        self.store
            .lock()
            .map_err(|_| anyhow!("override store lock poisoned"))?
            .address_for_matrix_handle(matrix_handle)
    }

    pub async fn get_candidates(&self) -> Result<Vec<Candidate>> {
        let mut candidates = self.chain.candidates_raw().await?;
        for candidate in &mut candidates {
            if let Some(handle) = self.get_matrix_handle(&candidate.address_or_handle).await? {
                candidate.address_or_handle = handle;
            }
        }
        Ok(candidates)
    }

    pub async fn get_bids(&self) -> Result<Vec<Bid>> {
        let mut bids = self.chain.bids_raw().await?;
        for bid in &mut bids {
            if let Some(handle) = self.get_matrix_handle(&bid.address_or_handle).await? {
                bid.address_or_handle = handle;
            }
        }
        Ok(bids)
    }

    pub async fn get_defending(&self) -> Result<Defender> {
        let mut defender = self.chain.defending_raw().await?;
        if let Some(address) = defender.address_or_handle.clone() {
            if let Some(handle) = self.get_matrix_handle(&address).await? {
                defender.address_or_handle = Some(handle);
            }
        }
        if let Some(address) = defender.skeptic.clone() {
            if let Some(handle) = self.get_matrix_handle(&address).await? {
                defender.skeptic = Some(handle);
            }
        }
        Ok(defender)
    }

    pub async fn get_candidate_skeptic(&self) -> Result<Option<String>> {
        let Some(skeptic) = self.chain.candidate_skeptic().await? else {
            return Ok(None);
        };
        Ok(self.get_matrix_handle(&skeptic).await?.or(Some(skeptic)))
    }

    pub async fn get_member_state(&self, address: &str) -> Result<MemberState> {
        if self.is_member(address).await? {
            Ok(MemberState::Member)
        } else if self.is_suspended_member(address).await? {
            Ok(MemberState::SuspendedMember)
        } else if self.is_candidate(address).await? {
            Ok(MemberState::Candidate)
        } else {
            Ok(MemberState::NonMember)
        }
    }

    pub async fn get_member_info(&self, address: &str) -> Result<MemberInfo> {
        Ok(MemberInfo {
            address: address.to_owned(),
            state: self.get_member_state(address).await?,
            element_handle: self.get_matrix_handle(address).await?,
            strikes: self.chain.strikes(address).await?,
            is_founder: self.is_founder(address).await?,
            is_defender: self.is_defender(address).await?,
        })
    }

    pub async fn get_candidate_period(&self) -> Result<CandidatePeriod> {
        Ok(CandidatePeriod::from_block(
            self.chain.block_number().await?,
        ))
    }

    pub async fn get_block_number(&self) -> Result<u64> {
        self.chain.block_number().await
    }

    pub async fn get_head_address(&self) -> Result<Option<String>> {
        self.chain.head_address().await
    }

    async fn is_member(&self, address: &str) -> Result<bool> {
        Ok(self
            .chain
            .member_addresses()
            .await?
            .iter()
            .any(|a| a == address))
    }

    async fn is_suspended_member(&self, address: &str) -> Result<bool> {
        Ok(self
            .chain
            .suspended_member_addresses()
            .await?
            .iter()
            .any(|a| a == address))
    }

    async fn is_candidate(&self, address: &str) -> Result<bool> {
        Ok(self
            .chain
            .candidates_raw()
            .await?
            .iter()
            .any(|c| c.address_or_handle == address))
    }

    async fn is_founder(&self, address: &str) -> Result<bool> {
        Ok(self.chain.founder().await?.as_deref() == Some(address))
    }

    async fn is_defender(&self, address: &str) -> Result<bool> {
        Ok(self
            .chain
            .defending_raw()
            .await?
            .address_or_handle
            .as_deref()
            == Some(address))
    }
}

#[derive(Clone)]
pub struct SubxtKusama {
    api: OnlineClient<PolkadotConfig>,
}

impl SubxtKusama {
    pub async fn connect(url: &str) -> Result<Self> {
        let rpc_client = if url.starts_with("ws://") || url.starts_with("http://") {
            RpcClient::from_insecure_url(url).await
        } else {
            RpcClient::from_url(url).await
        }
        .with_context(|| format!("connecting to Kusama RPC {url}"))?;

        let backend = LegacyBackend::<PolkadotConfig>::builder().build(rpc_client);
        let api = OnlineClient::<PolkadotConfig>::from_backend(Arc::new(backend))
            .await
            .with_context(|| format!("initializing legacy RPC client for {url}"))?;

        Ok(Self { api })
    }

    async fn fetch(&self, pallet: &str, entry: &str) -> Result<Option<Value>> {
        let address = storage::<Vec<Value>, Value>(pallet, entry);
        Ok(self
            .api
            .at_current_block()
            .await?
            .storage()
            .try_fetch(address, Vec::<Value>::new())
            .await?
            .map(|value| value.decode())
            .transpose()?)
    }

    async fn fetch_keyed(&self, pallet: &str, entry: &str, address: &str) -> Result<Option<Value>> {
        let account = decode_account_id(address).ok_or_else(|| anyhow!("invalid SS58 address"))?;
        let storage_address = storage::<Vec<Value>, Value>(pallet, entry);
        Ok(self
            .api
            .at_current_block()
            .await?
            .storage()
            .try_fetch(storage_address, vec![Value::from_bytes(account)])
            .await?
            .map(|value| value.decode())
            .transpose()?)
    }

    async fn map_addresses(&self, pallet: &str, entry: &str) -> Result<Vec<String>> {
        let address = storage::<Vec<Value>, Value>(pallet, entry);
        let at_block = self.api.at_current_block().await?;
        let storage = at_block.storage();
        let mut iter = storage.iter(address, Vec::<Value>::new()).await?;
        let mut addresses = Vec::new();
        while let Some(item) = iter.next().await {
            let pair = item?;
            if pair.key_bytes().len() >= 32 {
                let mut account = [0u8; 32];
                account.copy_from_slice(&pair.key_bytes()[pair.key_bytes().len() - 32..]);
                addresses.push(encode_account_id(&account, KUSAMA_SS58_PREFIX));
            }
        }
        Ok(addresses)
    }

    pub async fn watch_society_events<F, Fut>(&self, mut on_event: F) -> Result<()>
    where
        F: FnMut(SocietyEvent) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let mut blocks = self.api.stream_best_blocks().await?;
        while let Some(block) = blocks.next().await {
            let block = block?;
            let block_number = block.number();
            let block_hash = block.hash().0;
            let at_block = block.at().await?;
            let events = at_block.events().fetch().await?;
            for event in events.iter() {
                let event = event?;
                if event.pallet_name() != "Society" {
                    continue;
                }
                let event_name = event.event_name();
                let event_index = event.index();
                let values = event.decode_fields_unchecked_as::<Value>()?;
                let json = value_to_json(values)?;
                let Some(society_event) =
                    parse_society_event(block_number, block_hash, event_index, event_name, &json)
                else {
                    continue;
                };
                info!(?society_event, "observed society event");
                on_event(society_event).await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ChainData for SubxtKusama {
    async fn bids_raw(&self) -> Result<Vec<Bid>> {
        let Some(value) = self.fetch("Society", "Bids").await? else {
            return Ok(Vec::new());
        };
        let json = value_to_json(value)?;
        let Some(items) = json.as_array() else {
            return Ok(Vec::new());
        };
        let mut bids = Vec::new();
        for item in items {
            let Some(account) = find_account_ids(item).into_iter().next() else {
                continue;
            };
            bids.push(Bid {
                address_or_handle: encode_account_id(&account, KUSAMA_SS58_PREFIX),
                bid_plancks: find_u128_by_key(item, "value").unwrap_or_default(),
            });
        }
        Ok(bids)
    }

    async fn member_addresses(&self) -> Result<Vec<String>> {
        self.map_addresses("Society", "Members").await
    }

    async fn suspended_member_addresses(&self) -> Result<Vec<String>> {
        self.map_addresses("Society", "SuspendedMembers").await
    }

    async fn candidates_raw(&self) -> Result<Vec<Candidate>> {
        let address = storage::<Vec<Value>, Value>("Society", "Candidates");
        let at_block = self.api.at_current_block().await?;
        let storage = at_block.storage();
        let mut iter = storage.iter(address, Vec::<Value>::new()).await?;
        let mut candidates = Vec::new();
        while let Some(item) = iter.next().await {
            let pair = item?;
            let Some(account) = account_from_key(pair.key_bytes()) else {
                continue;
            };
            let json = value_to_json(pair.value().decode()?)?;
            candidates.push(Candidate {
                address_or_handle: encode_account_id(&account, KUSAMA_SS58_PREFIX),
                bid_plancks: find_u128_by_key(&json, "bid").unwrap_or_default(),
                tally: Tally {
                    approvals: find_u64_by_key(&json, "approvals").unwrap_or_default(),
                    rejections: find_u64_by_key(&json, "rejections").unwrap_or_default(),
                },
            });
        }
        Ok(candidates)
    }

    async fn strikes(&self, address: &str) -> Result<u64> {
        let Some(value) = self.fetch_keyed("Society", "Members", address).await? else {
            return Ok(0);
        };
        Ok(find_u64_by_key(&value_to_json(value)?, "strikes").unwrap_or_default())
    }

    async fn defending_raw(&self) -> Result<Defender> {
        let Some(value) = self.fetch("Society", "Defending").await? else {
            return Ok(Defender {
                address_or_handle: None,
                skeptic: None,
                tally: Tally {
                    approvals: 0,
                    rejections: 0,
                },
            });
        };
        let json = value_to_json(value)?;
        let accounts = find_account_ids(&json);
        Ok(Defender {
            address_or_handle: accounts
                .first()
                .map(|a| encode_account_id(a, KUSAMA_SS58_PREFIX)),
            skeptic: accounts
                .get(1)
                .map(|a| encode_account_id(a, KUSAMA_SS58_PREFIX)),
            tally: Tally {
                approvals: find_u64_by_key(&json, "approvals").unwrap_or_default(),
                rejections: find_u64_by_key(&json, "rejections").unwrap_or_default(),
            },
        })
    }

    async fn head_address(&self) -> Result<Option<String>> {
        Ok(self
            .fetch("Society", "Head")
            .await?
            .and_then(|value| value_to_json(value).ok())
            .and_then(|json| find_account_ids(&json).into_iter().next())
            .map(|account| encode_account_id(&account, KUSAMA_SS58_PREFIX)))
    }

    async fn candidate_skeptic(&self) -> Result<Option<String>> {
        Ok(self
            .fetch("Society", "Skeptic")
            .await?
            .and_then(|value| value_to_json(value).ok())
            .and_then(|json| find_account_ids(&json).into_iter().next())
            .map(|account| encode_account_id(&account, KUSAMA_SS58_PREFIX)))
    }

    async fn founder(&self) -> Result<Option<String>> {
        Ok(self
            .fetch("Society", "Founder")
            .await?
            .and_then(|value| value_to_json(value).ok())
            .and_then(|json| find_account_ids(&json).into_iter().next())
            .map(|account| encode_account_id(&account, KUSAMA_SS58_PREFIX)))
    }

    async fn block_number(&self) -> Result<u64> {
        Ok(self
            .fetch("System", "Number")
            .await?
            .and_then(|value| value.as_u128())
            .unwrap_or_default() as u64)
    }

    async fn identity_matrix_handle(&self, address: &str) -> Result<Option<String>> {
        let value = match self.fetch_keyed("Identity", "IdentityOf", address).await {
            Ok(value) => value,
            Err(error) if is_missing_storage_metadata(&error, "Identity", "IdentityOf") => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let Some(value) = value else {
            return Ok(None);
        };
        let json = value_to_json(value)?;
        Ok(find_riot_raw(&json))
    }
}

fn parse_society_event(
    block_number: u64,
    block_hash: [u8; 32],
    event_index: u32,
    event_name: &str,
    json: &JsonValue,
) -> Option<SocietyEvent> {
    match event_name {
        "Bid" => {
            let account = find_account_ids(json).into_iter().next()?;
            Some(SocietyEvent::Bid {
                id: SocietyEventId {
                    block_hash,
                    event_index,
                    kind: SocietyEventKind::Bid,
                },
                block_number,
                address: encode_account_id(&account, KUSAMA_SS58_PREFIX),
                bid_plancks: find_u128_by_key(json, "offer").unwrap_or_default(),
            })
        }
        "Unbid" => {
            let account = find_account_ids(json).into_iter().next()?;
            Some(SocietyEvent::Unbid {
                id: SocietyEventId {
                    block_hash,
                    event_index,
                    kind: SocietyEventKind::Unbid,
                },
                block_number,
                address: encode_account_id(&account, KUSAMA_SS58_PREFIX),
            })
        }
        _ => None,
    }
}

fn is_missing_storage_metadata(error: &anyhow::Error, pallet: &str, entry: &str) -> bool {
    if let Some(error) = error.downcast_ref::<StorageError>() {
        return storage_error_is_missing_metadata(error, pallet, entry);
    }

    matches!(
        error.downcast_ref::<SubxtError>(),
        Some(SubxtError::StorageError(error))
            if storage_error_is_missing_metadata(error, pallet, entry)
    )
}

fn storage_error_is_missing_metadata(error: &StorageError, pallet: &str, entry: &str) -> bool {
    match error {
        StorageError::PalletNameNotFound(name) => name == pallet,
        StorageError::StorageEntryNotFound {
            pallet_name,
            entry_name,
        } => pallet_name == pallet && entry_name == entry,
        StorageError::StorageInfoError(error) => {
            let message = error.to_string();
            message == format!("Pallet not found: {pallet}")
                || message == format!("Storage item not found: {entry} in pallet {pallet}")
        }
        _ => false,
    }
}

fn value_to_json(value: Value) -> Result<JsonValue> {
    serde_json::to_value(value).context("serializing dynamic chain value")
}

fn account_from_key(key: &[u8]) -> Option<[u8; 32]> {
    if key.len() < 32 {
        return None;
    }
    let mut account = [0u8; 32];
    account.copy_from_slice(&key[key.len() - 32..]);
    Some(account)
}

fn find_u64_by_key(value: &JsonValue, key: &str) -> Option<u64> {
    find_u128_by_key(value, key).and_then(|v| u64::try_from(v).ok())
}

fn find_u128_by_key(value: &JsonValue, key: &str) -> Option<u128> {
    match value {
        JsonValue::Object(map) => {
            if let Some(found) = map.get(key).and_then(json_to_u128) {
                return Some(found);
            }
            map.values().find_map(|child| find_u128_by_key(child, key))
        }
        JsonValue::Array(items) => items.iter().find_map(|child| find_u128_by_key(child, key)),
        _ => None,
    }
}

fn json_to_u128(value: &JsonValue) -> Option<u128> {
    match value {
        JsonValue::Number(number) => number.as_u64().map(u128::from),
        JsonValue::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn find_account_ids(value: &JsonValue) -> Vec<[u8; 32]> {
    let mut out = Vec::new();
    collect_account_ids(value, &mut out);
    out
}

fn collect_account_ids(value: &JsonValue, out: &mut Vec<[u8; 32]>) {
    match value {
        JsonValue::Array(items) if items.len() == 32 && items.iter().all(JsonValue::is_number) => {
            let mut account = [0u8; 32];
            for (idx, item) in items.iter().enumerate() {
                let Some(byte) = item.as_u64().and_then(|n| u8::try_from(n).ok()) else {
                    return;
                };
                account[idx] = byte;
            }
            out.push(account);
        }
        JsonValue::Array(items) => {
            for item in items {
                collect_account_ids(item, out);
            }
        }
        JsonValue::Object(map) => {
            for value in map.values() {
                collect_account_ids(value, out);
            }
        }
        _ => {}
    }
}

fn find_riot_raw(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::Object(map) => {
            if let Some(riot) = map.get("riot") {
                return find_raw_string(riot);
            }
            map.values().find_map(find_riot_raw)
        }
        JsonValue::Array(items) => items.iter().find_map(find_riot_raw),
        _ => None,
    }
}

fn find_raw_string(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::Object(map) => map
            .get("Raw")
            .and_then(JsonValue::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| map.values().find_map(find_raw_string)),
        JsonValue::String(text) => Some(text.to_owned()),
        JsonValue::Array(items) => items.iter().find_map(find_raw_string),
        _ => None,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::models::{CandidatePeriodKind, SocietyEventKind};
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    pub struct FakeChain {
        pub members: HashSet<String>,
        pub suspended: HashSet<String>,
        pub bids: Vec<Bid>,
        pub candidates: Vec<Candidate>,
        pub defender: Option<String>,
        pub defender_skeptic: Option<String>,
        pub strikes: HashMap<String, u64>,
        pub founder: Option<String>,
        pub head: Option<String>,
        pub candidate_skeptic: Option<String>,
        pub block_number: u64,
        pub identities: HashMap<String, String>,
    }

    #[async_trait]
    impl ChainData for FakeChain {
        async fn member_addresses(&self) -> Result<Vec<String>> {
            Ok(self.members.iter().cloned().collect())
        }

        async fn suspended_member_addresses(&self) -> Result<Vec<String>> {
            Ok(self.suspended.iter().cloned().collect())
        }

        async fn bids_raw(&self) -> Result<Vec<Bid>> {
            Ok(self.bids.clone())
        }

        async fn candidates_raw(&self) -> Result<Vec<Candidate>> {
            Ok(self.candidates.clone())
        }

        async fn strikes(&self, address: &str) -> Result<u64> {
            Ok(*self.strikes.get(address).unwrap_or(&0))
        }

        async fn defending_raw(&self) -> Result<Defender> {
            Ok(Defender {
                address_or_handle: self.defender.clone(),
                skeptic: self.defender_skeptic.clone(),
                tally: Tally {
                    approvals: 1,
                    rejections: 2,
                },
            })
        }

        async fn head_address(&self) -> Result<Option<String>> {
            Ok(self.head.clone())
        }

        async fn candidate_skeptic(&self) -> Result<Option<String>> {
            Ok(self.candidate_skeptic.clone())
        }

        async fn founder(&self) -> Result<Option<String>> {
            Ok(self.founder.clone())
        }

        async fn block_number(&self) -> Result<u64> {
            Ok(self.block_number)
        }

        async fn identity_matrix_handle(&self, address: &str) -> Result<Option<String>> {
            Ok(self.identities.get(address).cloned())
        }
    }

    const MEMBER: &str = "FUfBKr2pDxKrxmExGp4hjU6St4BDgffzKcyAqv6pruGnez1";
    const CANDIDATE: &str = "G75yJUM2TveDikvysHHW5XhkP35gXqDAsgRLYQTh3gVDir9";
    const SUSPENDED: &str = "J9c2fcmRhhNaJAxA8yLMkxap7PEWuYc1UaaTqxunfKscjG3";
    const DEFENDER: &str = "DGE8ATd2NaitqX4jdvZNXFNMmY9Qui6swnfoheCiz7efWGG";

    fn test_store() -> OverrideStore {
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        OverrideStore::open(path).unwrap()
    }

    #[tokio::test]
    async fn society_resolves_member_states_and_info() {
        let mut chain = FakeChain::default();
        chain.members.insert(MEMBER.to_owned());
        chain.suspended.insert(SUSPENDED.to_owned());
        chain.candidates.push(Candidate {
            address_or_handle: CANDIDATE.to_owned(),
            bid_plancks: 1,
            tally: Tally {
                approvals: 1,
                rejections: 0,
            },
        });
        chain.founder = Some(MEMBER.to_owned());
        chain.defender = Some(DEFENDER.to_owned());
        chain.strikes.insert(MEMBER.to_owned(), 2);
        chain
            .identities
            .insert(MEMBER.to_owned(), "@member:matrix.org".to_owned());

        let society = Society::new(chain, test_store());

        assert_eq!(
            society.get_member_state(MEMBER).await.unwrap(),
            MemberState::Member
        );
        assert_eq!(
            society.get_member_state(SUSPENDED).await.unwrap(),
            MemberState::SuspendedMember
        );
        assert_eq!(
            society.get_member_state(CANDIDATE).await.unwrap(),
            MemberState::Candidate
        );
        assert_eq!(
            society.get_member_state("unknown").await.unwrap(),
            MemberState::NonMember
        );

        let info = society.get_member_info(MEMBER).await.unwrap();
        assert_eq!(info.element_handle.as_deref(), Some("@member:matrix.org"));
        assert_eq!(info.strikes, 2);
        assert!(info.is_founder);
        assert!(!info.is_defender);
    }

    #[tokio::test]
    async fn society_prefers_db_overrides_for_handles() {
        let mut chain = FakeChain::default();
        chain
            .identities
            .insert(MEMBER.to_owned(), "@onchain:matrix.org".to_owned());
        chain.bids.push(Bid {
            address_or_handle: MEMBER.to_owned(),
            bid_plancks: 123,
        });
        chain.candidate_skeptic = Some(MEMBER.to_owned());
        chain.defender = Some(MEMBER.to_owned());
        chain.defender_skeptic = Some(MEMBER.to_owned());
        let society = Society::new(chain, test_store());

        assert!(
            society
                .set_matrix_handle(MEMBER, "@override:matrix.org")
                .unwrap()
        );
        assert!(
            !society
                .set_matrix_handle("bad", "@override:matrix.org")
                .unwrap()
        );
        assert!(!society.set_matrix_handle(MEMBER, "bad").unwrap());

        assert_eq!(
            society.get_matrix_handle(MEMBER).await.unwrap().as_deref(),
            Some("@override:matrix.org")
        );
        assert_eq!(
            society.get_bids().await.unwrap(),
            vec![Bid {
                address_or_handle: "@override:matrix.org".to_owned(),
                bid_plancks: 123,
            }]
        );
        assert_eq!(
            society.get_candidate_skeptic().await.unwrap().as_deref(),
            Some("@override:matrix.org")
        );
        assert_eq!(
            society
                .get_defending()
                .await
                .unwrap()
                .address_or_handle
                .as_deref(),
            Some("@override:matrix.org")
        );
        assert_eq!(
            society.get_defending().await.unwrap().skeptic.as_deref(),
            Some("@override:matrix.org")
        );
    }

    #[tokio::test]
    async fn society_calculates_candidate_period_from_chain_block() {
        let mut chain = FakeChain {
            block_number: 72_001,
            ..Default::default()
        };
        let society = Society::new(chain, test_store());
        let period = society.get_candidate_period().await.unwrap();
        assert_eq!(period.kind, CandidatePeriodKind::Claim);
        assert_eq!(period.voting_blocks_left, 0);

        chain = FakeChain {
            block_number: 100_799,
            ..Default::default()
        };
        let society = Society::new(chain, test_store());
        assert_eq!(
            society
                .get_candidate_period()
                .await
                .unwrap()
                .claim_blocks_left,
            1
        );
    }

    #[test]
    fn json_helpers_find_nested_values() {
        let account = [7u8; 32];
        let json = serde_json::json!({
            "outer": [
                {"bid": "1234567890123"},
                {"tally": {"approvals": 4, "rejections": "2"}},
                {"who": account},
                {"identity": {"info": {"riot": {"Raw": "@riot:matrix.org"}}}}
            ]
        });

        assert_eq!(find_u128_by_key(&json, "bid"), Some(1_234_567_890_123));
        assert_eq!(find_u64_by_key(&json, "approvals"), Some(4));
        assert_eq!(find_u64_by_key(&json, "rejections"), Some(2));
        assert_eq!(find_account_ids(&json), vec![account]);
        assert_eq!(find_riot_raw(&json).as_deref(), Some("@riot:matrix.org"));
        assert_eq!(find_u128_by_key(&json, "missing"), None);
    }

    #[test]
    fn account_from_key_uses_last_32_bytes() {
        let mut key = vec![1, 2, 3];
        key.extend([9u8; 32]);
        assert_eq!(account_from_key(&key), Some([9u8; 32]));
        assert_eq!(account_from_key(&[1, 2, 3]), None);
    }

    #[test]
    fn parses_society_bid_and_unbid_events() {
        let account = [7u8; 32];
        let block_hash = [9u8; 32];
        let json = serde_json::json!({"who": account, "offer": "5000000000000"});

        let bid = parse_society_event(42, block_hash, 3, "Bid", &json).unwrap();
        assert!(matches!(
            bid,
            SocietyEvent::Bid {
                block_number: 42,
                bid_plancks: 5_000_000_000_000,
                ..
            }
        ));
        assert_eq!(bid.id().event_index, 3);
        assert_eq!(bid.id().kind, SocietyEventKind::Bid);

        let unbid = parse_society_event(
            42,
            block_hash,
            4,
            "Unbid",
            &serde_json::json!({"who": account}),
        )
        .unwrap();
        assert!(matches!(
            unbid,
            SocietyEvent::Unbid {
                block_number: 42,
                ..
            }
        ));
        assert_eq!(unbid.id().event_index, 4);
        assert_eq!(unbid.id().kind, SocietyEventKind::Unbid);
        assert!(parse_society_event(42, block_hash, 5, "Voted", &json).is_none());
    }

    #[test]
    fn classifies_missing_optional_storage_metadata() {
        assert!(storage_error_is_missing_metadata(
            &StorageError::PalletNameNotFound("Identity".to_owned()),
            "Identity",
            "IdentityOf"
        ));
        assert!(storage_error_is_missing_metadata(
            &StorageError::StorageEntryNotFound {
                pallet_name: "Identity".to_owned(),
                entry_name: "IdentityOf".to_owned(),
            },
            "Identity",
            "IdentityOf"
        ));
        assert!(!storage_error_is_missing_metadata(
            &StorageError::PalletNameNotFound("System".to_owned()),
            "Identity",
            "IdentityOf"
        ));
    }
}
