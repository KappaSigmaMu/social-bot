use crate::models::{Candidate, CandidatePeriod, Defender, MemberInfo, MemberState, Tally};
use crate::ss58::{decode_account_id, encode_account_id, is_valid_address, is_valid_matrix_handle};
use crate::store::OverrideStore;
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};
use subxt::dynamic::{Value, storage};
use subxt::{OnlineClient, PolkadotConfig};

const KUSAMA_SS58_PREFIX: u16 = 2;

#[async_trait]
pub trait ChainData: Send + Sync {
    async fn member_addresses(&self) -> Result<Vec<String>>;
    async fn suspended_member_addresses(&self) -> Result<Vec<String>>;
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

pub struct SubxtKusama {
    api: OnlineClient<PolkadotConfig>,
}

impl SubxtKusama {
    pub async fn connect(url: &str) -> Result<Self> {
        Ok(Self {
            api: OnlineClient::<PolkadotConfig>::from_url(url)
                .await
                .with_context(|| format!("connecting to Kusama RPC {url}"))?,
        })
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
}

#[async_trait]
impl ChainData for SubxtKusama {
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
        let Some(value) = self.fetch_keyed("Identity", "IdentityOf", address).await? else {
            return Ok(None);
        };
        let json = value_to_json(value)?;
        Ok(find_riot_raw(&json))
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
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    pub struct FakeChain {
        pub members: HashSet<String>,
        pub suspended: HashSet<String>,
        pub candidates: Vec<Candidate>,
        pub defender: Option<String>,
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

        async fn candidates_raw(&self) -> Result<Vec<Candidate>> {
            Ok(self.candidates.clone())
        }

        async fn strikes(&self, _address: &str) -> Result<u64> {
            Ok(0)
        }

        async fn defending_raw(&self) -> Result<Defender> {
            Ok(Defender {
                address_or_handle: self.defender.clone(),
                skeptic: None,
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
}
