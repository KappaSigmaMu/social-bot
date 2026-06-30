use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberState {
    Member,
    Candidate,
    SuspendedMember,
    NonMember,
}

impl fmt::Display for MemberState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Member => write!(f, "member"),
            Self::Candidate => write!(f, "candidate"),
            Self::SuspendedMember => write!(f, "suspended member"),
            Self::NonMember => write!(f, "non-member"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidatePeriodKind {
    Voting,
    Claim,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePeriod {
    pub kind: CandidatePeriodKind,
    pub voting_blocks_left: u64,
    pub claim_blocks_left: u64,
}

impl CandidatePeriod {
    pub const VOTE_PERIOD_BLOCKS: u64 = 72_000;
    pub const CLAIM_PERIOD_BLOCKS: u64 = 28_800;
    pub const SECONDS_PER_BLOCK: u64 = 6;

    pub fn from_block(block: u64) -> Self {
        let cycle = Self::VOTE_PERIOD_BLOCKS + Self::CLAIM_PERIOD_BLOCKS;
        let position = block % cycle;

        if position < Self::VOTE_PERIOD_BLOCKS {
            Self {
                kind: CandidatePeriodKind::Voting,
                voting_blocks_left: Self::VOTE_PERIOD_BLOCKS - position,
                claim_blocks_left: Self::CLAIM_PERIOD_BLOCKS,
            }
        } else {
            Self {
                kind: CandidatePeriodKind::Claim,
                voting_blocks_left: 0,
                claim_blocks_left: cycle - position,
            }
        }
    }

    pub fn voting_time_left(&self) -> Duration {
        Duration::from_secs(self.voting_blocks_left * Self::SECONDS_PER_BLOCK)
    }

    pub fn claim_time_left(&self) -> Duration {
        Duration::from_secs(self.claim_blocks_left * Self::SECONDS_PER_BLOCK)
    }

    pub fn challenge_blocks_left(&self) -> u64 {
        self.voting_blocks_left + self.claim_blocks_left
    }

    pub fn challenge_time_left(&self) -> Duration {
        Duration::from_secs(self.challenge_blocks_left() * Self::SECONDS_PER_BLOCK)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tally {
    pub approvals: u64,
    pub rejections: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub address_or_handle: String,
    pub bid_plancks: u128,
    pub tally: Tally,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Bid {
    pub address_or_handle: String,
    pub bid_plancks: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocietyEventKind {
    Bid,
    Unbid,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SocietyEventId {
    pub block_hash: [u8; 32],
    pub event_index: u32,
    pub kind: SocietyEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocietyEvent {
    Bid {
        id: SocietyEventId,
        block_number: u64,
        address: String,
        bid_plancks: u128,
    },
    Unbid {
        id: SocietyEventId,
        block_number: u64,
        address: String,
    },
}

impl SocietyEvent {
    pub fn id(&self) -> &SocietyEventId {
        match self {
            Self::Bid { id, .. } | Self::Unbid { id, .. } => id,
        }
    }
}

pub struct SeenSocietyEvents {
    seen: std::sync::Mutex<std::collections::HashSet<SocietyEventId>>,
}

impl SeenSocietyEvents {
    pub fn new() -> Self {
        Self {
            seen: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// Returns `true` when this event has not been announced yet.
    pub fn mark_seen(&self, id: &SocietyEventId) -> bool {
        self.seen
            .lock()
            .expect("seen society events lock poisoned")
            .insert(id.clone())
    }
}

impl Default for SeenSocietyEvents {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Defender {
    pub address_or_handle: Option<String>,
    pub skeptic: Option<String>,
    pub tally: Tally,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInfo {
    pub address: String,
    pub state: MemberState,
    pub element_handle: Option<String>,
    pub strikes: u64,
    pub is_founder: bool,
    pub is_defender: bool,
}
