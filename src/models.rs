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
    Vouch,
    Unvouch,
    AutoUnbid,
    Inducted,
    Challenged,
    CandidateSuspended,
    MemberSuspended,
    SuspendedMemberJudgement,
    Elevated,
    Vote,
    DefenderVote,
}

impl SocietyEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bid => "Bid",
            Self::Unbid => "Unbid",
            Self::Vouch => "Vouch",
            Self::Unvouch => "Unvouch",
            Self::AutoUnbid => "AutoUnbid",
            Self::Inducted => "Inducted",
            Self::Challenged => "Challenged",
            Self::CandidateSuspended => "CandidateSuspended",
            Self::MemberSuspended => "MemberSuspended",
            Self::SuspendedMemberJudgement => "SuspendedMemberJudgement",
            Self::Elevated => "Elevated",
            Self::Vote => "Vote",
            Self::DefenderVote => "DefenderVote",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SocietyEventId {
    pub block_hash: [u8; 32],
    pub event_index: u32,
    pub kind: SocietyEventKind,
}

impl SocietyEventId {
    /// Stable storage key: hex block hash + event index + kind name.
    pub fn event_key(&self) -> (String, u32, &'static str) {
        (
            hex::encode(self.block_hash),
            self.event_index,
            self.kind.as_str(),
        )
    }
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
    Vouch {
        id: SocietyEventId,
        block_number: u64,
        candidate: String,
        offer_plancks: u128,
        voucher: String,
    },
    Unvouch {
        id: SocietyEventId,
        block_number: u64,
        candidate: String,
    },
    AutoUnbid {
        id: SocietyEventId,
        block_number: u64,
        candidate: String,
    },
    Inducted {
        id: SocietyEventId,
        block_number: u64,
        primary: String,
        candidates: Vec<String>,
    },
    Challenged {
        id: SocietyEventId,
        block_number: u64,
        member: String,
    },
    CandidateSuspended {
        id: SocietyEventId,
        block_number: u64,
        candidate: String,
    },
    MemberSuspended {
        id: SocietyEventId,
        block_number: u64,
        member: String,
    },
    SuspendedMemberJudgement {
        id: SocietyEventId,
        block_number: u64,
        who: String,
        judged: bool,
    },
    Elevated {
        id: SocietyEventId,
        block_number: u64,
        member: String,
        rank: u64,
    },
    Vote {
        id: SocietyEventId,
        block_number: u64,
        candidate: String,
        voter: String,
        approve: bool,
    },
    DefenderVote {
        id: SocietyEventId,
        block_number: u64,
        voter: String,
        approve: bool,
    },
}

impl SocietyEvent {
    pub fn id(&self) -> &SocietyEventId {
        match self {
            Self::Bid { id, .. }
            | Self::Unbid { id, .. }
            | Self::Vouch { id, .. }
            | Self::Unvouch { id, .. }
            | Self::AutoUnbid { id, .. }
            | Self::Inducted { id, .. }
            | Self::Challenged { id, .. }
            | Self::CandidateSuspended { id, .. }
            | Self::MemberSuspended { id, .. }
            | Self::SuspendedMemberJudgement { id, .. }
            | Self::Elevated { id, .. }
            | Self::Vote { id, .. }
            | Self::DefenderVote { id, .. } => id,
        }
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
